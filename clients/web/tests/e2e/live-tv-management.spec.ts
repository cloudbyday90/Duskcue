/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { test, expect } from './fixtures';
import { managementIds, fixtureAssignment } from '../fixtures/live-tv-management.js';
import { profileIds } from '../fixtures/catalog.js';
import { keyboardReach, noHorizontalOverflow, textContrast } from './accessibility-journey';

test.use({ scenarioOptions: { userRole: 'owner' } });

function unlock(api) { api.household.enabled = true; api.household.trustedPassword = true; api.household.grant = new Date(Date.now() + 300000).toISOString(); }
async function choose(page, target = managementIds.target) { await page.goto('/settings/live-tv'); await page.getByLabel('Choose a household', { exact: true }).selectOption(target); }

test('administrator creates, configures and approves write-only sources without exposing saved values', async ({ page, api }) => {
    unlock(api); await choose(page);
    await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://dispatcharr.invalid/m3u/?password=PRIVATE_SOURCE');
    await page.getByLabel('Dispatcharr XMLTV URL', { exact: true }).fill('http://dispatcharr.invalid/xmltv/?password=PRIVATE_SOURCE');
    await page.getByRole('button', { name: 'Create assignment', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeEnabled();
    await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveValue('');
    await page.getByLabel('Household Live TV allowance', { exact: true }).check();
    await page.getByLabel('Allow the household owner to manage standard-profile access', { exact: true }).check();
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Approve destination', exact: true })).toBeEnabled();
    await page.getByLabel('Expected feed origin', { exact: true }).fill('http://dispatcharr.invalid');
    await page.getByLabel('Approved IP addresses, one per line in connection order', { exact: true }).fill('127.0.0.1\n127.0.0.2');
    await page.getByRole('button', { name: 'Approve destination', exact: true }).click();
    await expect(page.getByText('Playlist: 2 approved address choices', { exact: true })).toBeVisible();
    await expect(page.getByLabel('Expected feed origin', { exact: true })).toHaveValue('');
    expect(await page.evaluate(() => JSON.stringify(Object.entries(localStorage)))).not.toContain('PRIVATE_SOURCE');
    expect(api.management.writes.map((entry) => entry.operation)).toEqual(['create', 'configure', 'approve_destination']);
});

test('self-household save revokes management without signing out viewing or continuing the next step', async ({ page, api }) => {
    unlock(api); api.management.assignments.set(api.user.id, fixtureAssignment(api.user.id)); await choose(page, api.user.id);
    await page.getByLabel('Household Live TV allowance', { exact: true }).check();
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
    expect(api.household.grant).toBeNull(); expect(api.management.writes).toHaveLength(1);
    await page.getByRole('link', { name: 'Unlock household management', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/household$/);
});

test('rotation and replacement have distinct identity and confirmation behavior', async ({ page, api }) => {
    unlock(api); api.management.assignments.set(managementIds.target, fixtureAssignment(managementIds.target)); await choose(page);
    const old = api.management.assignments.get(managementIds.target).assignment_id;
    const fill = async () => { await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://d.invalid/m3u/'); await page.getByLabel('Dispatcharr XMLTV URL', { exact: true }).fill('http://d.invalid/epg/'); };
    await fill(); await page.getByRole('button', { name: 'Rotate credentials', exact: true }).click();
    await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveValue('');
    expect(api.management.assignments.get(managementIds.target).assignment_id).toBe(old);
    await page.getByLabel('Source operation', { exact: true }).selectOption('replace_assignment'); await fill();
    await page.getByRole('button', { name: 'Replace assignment', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: 'Review action', exact: true });
    await expect(dialog).toContainText('Owner profile choices');
    await expect(dialog.getByRole('button', { name: 'Keep editing', exact: true })).toBeFocused();
    await page.keyboard.press('Escape'); expect(api.management.writes).toHaveLength(1);
    await page.getByRole('button', { name: 'Replace assignment', exact: true }).click();
    await dialog.getByRole('button', { name: 'Replace assignment', exact: true }).click();
    await expect.poll(() => api.management.assignments.get(managementIds.target).assignment_id).not.toBe(old);
});

test('operator enable and accepted refresh status stay separate from assignment and playback', async ({ page, api }) => {
    unlock(api); await page.goto('/settings/live-tv-control');
    await page.getByLabel('Allow manual imports', { exact: true }).check();
    await page.getByRole('button', { name: 'Save server refresh control', exact: true }).click();
    await expect.poll(() => api.management.control.enabled).toBe(true);
    api.management.assignments.set(managementIds.target, fixtureAssignment(managementIds.target, { enabled: true, playlist_destination_approved: true, guide_destination_approved: true, playlist_address_count: 2, guide_address_count: 1 }));
    await choose(page);
    await page.getByLabel('Playlist address choice', { exact: true }).selectOption('1'); await page.getByLabel('Guide address choice', { exact: true }).selectOption('0');
    await page.getByRole('button', { name: 'Start manual refresh', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Latest refresh succeeded' })).toBeVisible();
    expect(api.management.starts).toBe(1);
    await page.getByRole('button', { name: 'Check status', exact: true }).click(); expect(api.management.starts).toBe(1);
    expect(api.requests.some((entry) => entry.path.includes('/api/v1/api/v1/'))).toBe(false);
});

test('owner controls enforce delegation and leave Kids ineligible', async ({ page, api }) => {
    unlock(api); api.management.assignments.set(api.user.id, fixtureAssignment(api.user.id, { enabled: true, allow_owner_profile_access: true, allow_owner_live_limit: true }));
    await page.goto('/settings/household-live-tv');
    await expect(page.getByRole('checkbox', { name: 'Live TV access for Kids', exact: true })).toBeDisabled();
    await page.getByRole('checkbox', { name: 'Live TV access for Morgan', exact: true }).click();
    await expect.poll(() => api.management.profiles.get(profileIds.morgan)).toBe(true);
    await expect(page.getByRole('textbox', { name: 'Additional household Live TV ceiling', exact: true })).toBeEnabled();
    await page.getByRole('textbox', { name: 'Additional household Live TV ceiling', exact: true }).fill('2');
    await page.getByRole('button', { name: 'Save household ceiling', exact: true }).click();
    await expect.poll(() => api.management.assignments.get(api.user.id).owner_max_live_streams).toBe(2);
});

test('a stale resource disables writes and requires explicit reload without replay', async ({ page, api }) => {
    unlock(api); api.management.assignments.set(managementIds.target, fixtureAssignment(managementIds.target)); await choose(page);
    await page.getByLabel('Household Live TV allowance', { exact: true }).check();
    api.management.assignments.get(managementIds.target).revision += 1;
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('session or resource changed');
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toHaveCount(0);
    expect(api.management.writes).toHaveLength(0);
    await page.getByRole('button', { name: 'Reload metadata', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeEnabled();
});

test('dirty source navigation supports Escape, discard and logical focus', async ({ page, api }) => {
    unlock(api); await choose(page); await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://private.invalid/source');
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    const dialog = page.getByRole('dialog', { name: 'Review action', exact: true }); await expect(dialog).toBeVisible();
    await page.keyboard.press('Escape'); await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveValue('http://private.invalid/source');
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    await dialog.getByRole('button', { name: 'Discard changes', exact: true }).click(); await expect(page).toHaveURL(/\/dashboard$/);
    expect(api.management.writes).toHaveLength(0);
});

test('320px RTL management forms retain paste, focus and reflow', async ({ page, api }) => {
    unlock(api); await page.setViewportSize({ width: 320, height: 180 }); await page.goto('/ar/settings/live-tv');
    await page.getByLabel('Choose a household', { exact: true }).selectOption(managementIds.target);
    await expect(page.locator('html')).toHaveAttribute('dir', 'rtl'); await expect(page.locator('.management')).toHaveAttribute('lang', 'en');
    const input = page.getByLabel('Dispatcharr M3U URL', { exact: true }); await keyboardReach(page, input);
    expect((await textContrast(input)).ratio).toBeGreaterThanOrEqual(4.5);
    await noHorizontalOverflow(page); await page.screenshot({ path: test.info().outputPath('management-rtl-focus.png') });
});

test('definitive validation retains nonsensitive edits but an uncertain write blocks replay', async ({ page, api }) => {
    unlock(api); api.management.assignments.set(managementIds.target, fixtureAssignment(managementIds.target)); await choose(page);
    const cap = page.getByLabel('Administrator Live TV ceiling', { exact: true });
    await cap.fill('2'); api.management.refusal = { status: 422, title: 'IPTV_006' };
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('correct the supplied values'); await expect(cap).toHaveValue('2');
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeEnabled();
    api.management.refusal = null; let submissions = 0;
    await page.route(`**/live-tv/households/${managementIds.target}/assignment`, async (route) => {
        if (route.request().method() !== 'PATCH') return route.fallback();
        submissions += 1; api.management.assignments.get(managementIds.target).revision += 1;
        await route.fulfill({ status: 200, contentType: 'application/json', headers: { ETag: `"${'a'.repeat(64)}"` }, body: '{' });
    });
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('outcome could not be confirmed'); expect(submissions).toBe(1);
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toHaveCount(0);
    await page.getByRole('button', { name: 'Reload metadata', exact: true }).click();
    await expect(cap).toHaveValue(''); expect(submissions).toBe(1);
});

test('exact browser JSON source text preserves a server revision above 2^53', async ({ page, api }) => {
    unlock(api);
    await page.route('**/live-tv/manual-refresh-control', (route) => route.fulfill({ status: 200, contentType: 'application/json', headers: { ETag: `"${'b'.repeat(64)}"` }, body: `{"id":"${managementIds.control}","revision":9007199254740993,"enabled":false}` }));
    await page.goto('/settings/live-tv-control');
    await expect(page.getByText('Revision: 9007199254740993', { exact: true })).toBeVisible();
});

test('grant expiry clears source drafts and disables saving while viewing stays signed in', async ({ page, api }) => {
    unlock(api); api.household.grant = new Date(Date.now() + 8000).toISOString();
    await page.clock.install(); await choose(page);
    await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://private.invalid/draft');
    await page.clock.fastForward(9000);
    await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveValue('');
    await expect(page.getByRole('button', { name: 'Create assignment', exact: true })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
    expect(api.management.writes).toHaveLength(0);
});

test('declining a target change restores its selection, and clearing the target clears forms', async ({ page, api }) => {
    unlock(api); await choose(page); await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://private.invalid/draft');
    const picker = page.getByLabel('Choose a household', { exact: true }); await picker.selectOption(api.user.id);
    await expect(page.getByRole('dialog', { name: 'Review action', exact: true })).toBeVisible(); await page.keyboard.press('Escape');
    await expect(picker).toHaveValue(managementIds.target); await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveValue('http://private.invalid/draft');
    await picker.selectOption(''); await page.getByRole('dialog', { name: 'Review action', exact: true }).getByRole('button', { name: 'Discard changes', exact: true }).click();
    await expect(picker).toHaveValue(''); await expect(page.getByLabel('Dispatcharr M3U URL', { exact: true })).toHaveCount(0); expect(api.management.writes).toHaveLength(0);
});

test.describe('Kids isolation', () => { test.use({ scenarioOptions: { userRole: 'owner', activeProfileId: profileIds.kids } }); test('direct management routes send no management requests', async ({ page, api }) => {
    for (const path of ['/settings/live-tv', '/settings/household-live-tv', '/settings/live-tv-control']) { await page.goto(path); await expect(page.getByText('Live TV management requires a selected standard profile.', { exact: true })).toBeVisible(); }
    expect(api.management.writes).toHaveLength(0); expect(api.requests.some((entry) => entry.path === '/users' || entry.path.startsWith('/live-tv/households/') || entry.path === '/live-tv/manual-refresh-control' || entry.path === '/household-management/live-tv')).toBe(false);
}); });
