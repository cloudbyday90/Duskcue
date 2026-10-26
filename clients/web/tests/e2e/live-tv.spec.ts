/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';
import { keyboardReach, noHorizontalOverflow, textContrast } from './accessibility-journey';

const channel = (page) => page.locator('.channels button').first();
const channels = (page) => page.getByRole('navigation', { name: 'Channel pages', exact: true });
const programmes = (page) => page.getByRole('navigation', { name: 'Programme pages', exact: true });
const requests = (api) => api.requests.filter((entry) => entry.path.startsWith('/live-tv/catalog/'));

test('standard profiles discover server-authorized channels, replace pages and explicitly navigate six-hour windows', async ({ page, api }) => {
    await page.clock.setFixedTime(new Date('2026-11-01T05:00:00Z'));
    await page.goto('/dashboard');
    expect(api.user.capabilities).toEqual([]);
    await page.getByRole('link', { name: 'Live TV', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    await expect(page.locator('.channels button')).toHaveCount(20);
    await expect(page.locator('.programme')).toHaveCount(0);
    expect(requests(api)).toHaveLength(1);
    await channel(page).click();
    await expect(page.locator('#live-tv-guide')).toBeFocused();
    await expect(page.locator('.programme')).toHaveCount(20);
    const first = requests(api).at(-1).query;
    expect(Date.parse(first.to) - Date.parse(first.from)).toBe(6 * 60 * 60 * 1000);
    await programmes(page).getByRole('button', { name: 'Next', exact: true }).click();
    await expect(page.locator('.programme')).toHaveCount(5);
    await expect(programmes(page).getByRole('button', { name: 'Next', exact: true })).toBeFocused();
    await page.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(programmes(page)).toContainText('Page 1');
    await expect.poll(() => requests(api).at(-1).query.from).toBe(first.to);
    await page.getByRole('button', { name: 'Earlier', exact: true }).click();
    await expect.poll(() => requests(api).at(-1).query.from).toBe(first.from);
    await channels(page).getByRole('button', { name: 'Next', exact: true }).click();
    await expect(channel(page)).toContainText('Channel 21');
    await expect(page.locator('.programme')).toHaveCount(0);
    await channels(page).getByRole('button', { name: 'Previous', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    expect(requests(api).every((entry) => entry.query.limit === '20')).toBe(true);
    expect(new URL(page.url()).search).toBe('');
    await page.screenshot({ path: test.info().outputPath('live-tv-wide.png') });
});

test('supplied variants, raw ratings and incomplete timing remain escaped and explicit', async ({ page, api }) => {
    const external: string[] = [];
    page.on('request', (request) => { if (request.url().includes('private-provider.invalid')) external.push(request.url()); });
    await page.goto('/live-tv');
    await channel(page).click();
    const card = page.locator('.programme').first();
    await expect(card.getByRole('heading', { name: 'Evening bulletin <script>', exact: true })).toBeVisible();
    await expect(card.locator('h3 bdi')).toHaveAttribute('lang', 'en');
    await card.getByText('All supplied titles', { exact: true }).click();
    await expect(card.getByText('نشرة المساء', { exact: true })).toBeVisible();
    await expect(card.getByText('Bulletin 1', { exact: true })).toBeVisible();
    await card.getByText('Supplied ratings (unclassified)', { exact: true }).click();
    await expect(card.getByText('Unmapped-7', { exact: true })).toBeVisible();
    await card.getByText('Supplied timing details', { exact: true }).click();
    await expect(card).toContainText('.123456Z');
    await expect(card).toContainText('No complete instant supplied');
    await expect(card.locator('script, a, img')).toHaveCount(0);
    await page.getByRole('button', { name: 'Incomplete times', exact: true }).click();
    await expect(page.locator('.programme')).toHaveCount(3);
    await expect(page.getByRole('button', { name: 'Later', exact: true })).toHaveCount(0);
    expect(requests(api).at(-1).query).not.toHaveProperty('from');
    expect(external).toEqual([]);
});

test('409 clears all rendered evidence and needs a manual fresh read with logical focus recovery', async ({ page, api }) => {
    await page.goto('/live-tv'); await channel(page).click();
    await expect(page.locator('.programme')).toHaveCount(20);
    api.liveTv.status = 409;
    await programmes(page).getByRole('button', { name: 'Next', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'catalog or profile context changed' })).toBeVisible();
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'Live TV', exact: true })).toBeFocused();
    await expect(page.getByText('PRIVATE_UPSTREAM_ERROR')).toHaveCount(0);
    const before = requests(api).length;
    api.liveTv.status = 200;
    await page.getByRole('button', { name: 'Reload Live TV', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    expect(requests(api)).toHaveLength(before + 1);
    expect(requests(api).at(-1).query).not.toHaveProperty('cursor');
    await expect(page.locator('.programme')).toHaveCount(0);
});

test('temporary refusal honors Retry-After without automatic request replay', async ({ page, api }) => {
    api.liveTv.status = 503; api.liveTv.retryAfter = '2';
    await page.goto('/live-tv');
    const reload = page.getByRole('button', { name: 'Reload Live TV', exact: true });
    await expect(reload).toHaveAttribute('aria-disabled', 'true');
    await reload.click({ force: true });
    expect(requests(api)).toHaveLength(1);
    api.liveTv.status = 200;
    await expect(reload).toHaveAttribute('aria-disabled', 'false');
    expect(requests(api)).toHaveLength(1);
    await reload.click();
    await expect(channel(page)).toContainText('World & Culture');
});

test('malformed responses and unavailable access clear the old catalog', async ({ page, api }) => {
    await page.goto('/live-tv'); await channel(page).click();
    await expect(page.locator('.programme')).toHaveCount(20);
    api.liveTv.malformed = true;
    await page.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'could not be loaded' })).toBeVisible();
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
    api.liveTv.malformed = false; api.liveTv.status = 403;
    await page.getByRole('button', { name: 'Reload Live TV', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'unavailable for this profile' })).toBeVisible();
});

test('valid empty snapshots remain distinct from failures', async ({ page, api }) => {
    api.liveTv.channelCount = 0;
    await page.goto('/live-tv');
    await expect(page.getByText('No channels in this snapshot.', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Reload Live TV', exact: true })).toHaveCount(0);
});

test('a catalog 401 revalidates the current session without replaying or logging out a valid account', async ({ page, api }) => {
    await page.goto('/live-tv'); await channel(page).click();
    await expect(page.locator('.programme')).toHaveCount(20);
    const probes = api.requests.filter((entry) => entry.path === '/auth/session-context').length;
    api.liveTv.status = 401;
    await page.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'session needs verification' })).toBeVisible();
    await expect.poll(() => api.requests.filter((entry) => entry.path === '/auth/session-context').length).toBe(probes + 1);
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
    const before = requests(api).length;
    api.liveTv.status = 200;
    await page.getByRole('button', { name: 'Reload Live TV', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    expect(requests(api)).toHaveLength(before + 1);
    await expect(page.getByRole('heading', { name: 'Live TV', exact: true })).toBeFocused();
});

test('a confirmed expired current session clears catalog data and uses the existing sign-in flow', async ({ page, api }) => {
    await page.goto('/live-tv'); await channel(page).click();
    await expect(page.locator('.programme')).toHaveCount(20);
    api.liveTv.status = 401; api.authenticated = false;
    await page.getByRole('button', { name: 'Later', exact: true }).click();
    await expect(page).toHaveURL(/\/auth\/login/);
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
});

test.describe('Kids isolation', () => {
    test.use({ scenarioOptions: { activeProfileId: profileIds.kids } });
    test('has no navigation entry or catalog request even on direct entry', async ({ page, api }) => {
        await page.goto('/live-tv');
        await expect(page.getByText('Live TV is available only to standard viewing profiles.', { exact: true })).toBeVisible();
        await expect(page.getByRole('link', { name: 'Live TV', exact: true })).toHaveCount(0);
        expect(requests(api)).toHaveLength(0);
    });
});

test('profile switches and browser history start fresh memory-only views', async ({ page, api }) => {
    await page.goto('/live-tv');
    await channels(page).getByRole('button', { name: 'Next', exact: true }).click();
    await expect(channel(page)).toContainText('Channel 21');
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    await expect(page).toHaveURL(/\/dashboard$/);
    await page.goBack();
    await expect(channel(page)).toContainText('World & Culture');
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('button', { name: 'Morgan', exact: true }).click();
    await expect.poll(() => api.activeProfileId).toBe(profileIds.morgan);
    await expect(page).toHaveURL(/\/dashboard$/);
    await page.getByRole('link', { name: 'Live TV', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    await expect(page.locator('.programme')).toHaveCount(0);
    const storage = await page.evaluate(() => Object.entries(localStorage).filter(([key]) => /live.?tv|catalog|cursor/i.test(key)));
    expect(storage).toEqual([]);
});

test('late guide completion cannot republish across a profile switch', async ({ page, api }) => {
    let release;
    let started = false;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    await page.route(/\/live-tv\/catalog\/channels\/[^/]+\/programmes\?/, async (route) => { started = true; await gate; await route.fulfill({ contentType: 'application/json', body: '{"obsolete":true}' }).catch(() => {}); });
    await page.goto('/live-tv'); await channel(page).click();
    await expect.poll(() => started).toBe(true);
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('button', { name: 'Morgan', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard$/);
    release();
    await page.getByRole('link', { name: 'Live TV', exact: true }).click();
    await expect(channel(page)).toContainText('World & Culture');
    await expect(page.locator('.programme')).toHaveCount(0);
    await expect(page.getByText('obsolete', { exact: true })).toHaveCount(0);
});

test('a pending failure does not steal focus from shell navigation', async ({ page, api }) => {
    let release;
    let started = false;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    await page.route(/\/live-tv\/catalog\/channels\/[^/]+\/programmes\?/, async (route) => {
        started = true; await gate;
        await route.fulfill({ status: 409, contentType: 'application/problem+json', body: '{"status":409}' });
    });
    await page.goto('/live-tv'); await channel(page).click();
    await expect.poll(() => started).toBe(true);
    const menu = page.getByRole('button', { name: 'User menu', exact: true });
    await menu.click();
    release();
    await expect(page.getByRole('button', { name: 'Reload Live TV', exact: true })).toBeVisible();
    await expect(menu).toBeFocused();
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
});

test('320px RTL reflow, fallback language, native disclosure keyboard focus and contrast', async ({ page, api }) => {
    await page.setViewportSize({ width: 320, height: 480 });
    await page.goto('/ar/live-tv');
    await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
    await expect(page.locator('.live-tv')).toHaveAttribute('lang', 'en');
    await keyboardReach(page, channel(page));
    expect((await textContrast(channel(page))).ratio).toBeGreaterThanOrEqual(4.5);
    await page.keyboard.press('Enter');
    await expect(page.locator('#live-tv-guide')).toBeFocused();
    await keyboardReach(page, page.getByRole('button', { name: 'Scheduled programmes', exact: true }));
    await keyboardReach(page, page.getByRole('button', { name: 'Incomplete times', exact: true }));
    await page.keyboard.press('Enter');
    await expect(page.locator('.programme')).toHaveCount(3);
    const summary = page.locator('.programme').first().locator('summary').first();
    await keyboardReach(page, summary);
    await page.keyboard.press('Enter');
    await expect(page.locator('.programme').first().locator('details[open]').getByText('نشرة المساء', { exact: true })).toBeVisible();
    await noHorizontalOverflow(page);
    await page.screenshot({ path: test.info().outputPath('live-tv-rtl-narrow.png') });
});
