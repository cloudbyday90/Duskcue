/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';
import { keyboardReach, noHorizontalOverflow, textContrast } from './accessibility-journey';

test('standard profiles discover household management separately from server security', async ({ page, api }) => {
    await page.goto('/settings');
    await page.getByRole('link', { name: 'Household management', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Household management', exact: true })).toBeVisible();
    await expect(page.getByText('Owner-trusted passkeys: 0', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Verify owner password', exact: true })).toHaveCount(0);
    expect(api.requests.some((entry) => entry.path === '/auth/session-context')).toBe(true);
});

test('operator handoff plus existing password activates protection, rotates context and permits explicit lock', async ({ page, api }) => {
    await page.goto('/settings/household');
    await page.getByText('Enroll or recover with an operator handoff', { exact: true }).click();
    await page.getByLabel('Operator handoff', { exact: true }).fill(api.household.handoff);
    await page.locator('#owner-enrollment-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Enable owner protection', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
    expect(api.household.revision).toBe('1');
    await expect(page.locator('#owner-password').first()).toHaveValue('');
    await page.getByRole('button', { name: 'Lock management', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
    expect(api.household.grant).toBeNull();
    const storage = await page.evaluate(() => Object.entries(localStorage).map(([key, value]) => [key, value]));
    expect(JSON.stringify(storage)).not.toContain(api.household.password);
    expect(JSON.stringify(storage)).not.toContain('owner-handoff-fixture');
    expect(JSON.stringify(storage)).not.toContain('credential_revision');
});

test('a rejected owner password clears input and does not sign out viewing', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.goto('/settings/household');
    await page.locator('#owner-password').fill('wrong');
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('viewing session has not been signed out');
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
    await expect(page.getByText('PRIVATE_OWNER_REFUSAL')).toHaveCount(0);
    expect(api.requests.filter((entry) => entry.path === '/household-management/password')).toHaveLength(1);
    await page.getByRole('button', { name: 'Check status', exact: true }).click();
    await expect(page.locator('#owner-password')).toHaveValue('');
});

test('stale context and malformed proof never present management as verified', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.goto('/settings/household');
    await expect(page.locator('#owner-password')).toBeVisible();
    api.household.profileRevision = '2';
    await page.locator('#owner-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('context changed');
    await page.getByRole('button', { name: 'Check status', exact: true }).click();
    await expect(page.locator('#owner-password')).toBeVisible();
    await page.route('**/api/v1/household-management/password', (route) => route.fulfill({ contentType: 'application/json', body: '{"expires_at":"2030-01-01T00:00:00Z"}' }));
    await page.locator('#owner-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('could not be confirmed');
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toHaveCount(0);
});

test('an unconfirmed proof is not replayed when the user leaves the screen', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    let release;
    let started = false;
    const gate = new Promise<void>((resolve) => { release = resolve; });
    await page.route('**/api/v1/household-management/password', async (route) => { started = true; await gate; await route.fulfill({ status: 503, contentType: 'application/problem+json', body: '{"status":503}' }); });
    await page.goto('/settings/household');
    await page.locator('#owner-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect.poll(() => started).toBe(true);
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    await expect(page).toHaveURL(/\/dashboard$/);
    release();
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
});

test('bounded retry and an unknown lockout duration remain explicit', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.goto('/settings/household');
    await expect(page.locator('#owner-password')).toBeVisible();
    api.household.refusal = { status: 429, title: 'OWNER_003' };
    await page.locator('#owner-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('temporarily locked or busy');
    await expect(page.getByText(/Wait \d+ seconds/)).toHaveCount(0);
    expect(api.requests.filter((entry) => entry.path === '/household-management/password')).toHaveLength(1);
});

test('320px RTL authentication permits native keyboard controls, autofill and visible focus', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.setViewportSize({ width: 320, height: 180 });
    await page.goto('/ar/settings/household');
    await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
    await expect(page.locator('.household')).toHaveAttribute('lang', 'en');
    const password = page.locator('#owner-password');
    await expect(password).toHaveAttribute('autocomplete', 'current-password');
    await keyboardReach(page, password);
    expect((await textContrast(password)).ratio).toBeGreaterThanOrEqual(4.5);
    await password.fill(api.household.password);
    const submit = page.getByRole('button', { name: 'Verify owner password', exact: true });
    await keyboardReach(page, submit);
    await noHorizontalOverflow(page);
    await page.screenshot({ path: test.info().outputPath('household-rtl-focus.png') });
    await page.keyboard.press('Enter');
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
    await noHorizontalOverflow(page);
});

test('a metadata-only notice clears another tab and requires an explicit authoritative status read', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.goto('/settings/household');
    await expect(page.locator('#owner-password')).toBeVisible();
    await page.locator('#owner-password').fill(api.household.password);
    await page.evaluate(() => window.dispatchEvent(new StorageEvent('storage', { key: 'duskcue_session_context_notice', newValue: JSON.stringify({ origin: location.origin, nonce: 'other-tab-notice' }) })));
    await expect(page.locator('#owner-password')).toHaveCount(0);
    expect(api.requests.filter((entry) => entry.path === '/household-management/password')).toHaveLength(0);
    await page.getByRole('button', { name: 'Check status', exact: true }).click();
    await expect(page.locator('#owner-password')).toHaveValue('');
});

test('independently expired viewing authentication uses session recovery, without replaying owner proof', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    await page.goto('/settings/household');
    await expect(page.locator('#owner-password')).toBeVisible();
    api.authenticated = false;
    await page.locator('#owner-password').fill(api.household.password);
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page).toHaveURL(/\/auth\/login$/);
    expect(api.requests.filter((entry) => entry.path === '/household-management/password')).toHaveLength(1);
});

test('expired management presentation clears secret input without expiring viewing', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    api.household.grant = new Date(Date.now() + 3000).toISOString();
    await page.goto('/settings/household');
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
    await page.locator('#owner-password').fill('unsubmitted-sensitive-value');
    await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
    await expect(page.locator('#owner-password')).toHaveValue('');
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
});

test('grant evidence already expired in transit is presented as locked', async ({ page, api }) => {
    api.household.enabled = true; api.household.trustedPassword = true;
    api.household.grant = new Date(Date.now() - 1000).toISOString();
    await page.goto('/settings/household');
    await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toHaveCount(0);
});

test.describe('Kids household isolation', () => {
    test.use({ scenarioOptions: { activeProfileId: profileIds.kids } });
    test('direct entry has no controls or owner requests', async ({ page, api }) => {
        await page.goto('/settings/household');
        await expect(page.getByText('Household management requires a selected standard viewing profile.', { exact: true })).toBeVisible();
        expect(api.requests.some((entry) => entry.path.startsWith('/household-management/'))).toBe(false);
        await page.goto('/settings');
        await expect(page.getByRole('link', { name: 'Household management', exact: true })).toHaveCount(0);
    });
});
