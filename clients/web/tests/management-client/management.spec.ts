/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { readFile } from 'node:fs/promises';
import { test, expect } from '@playwright/test';

test('real protected setup, controlled paired import, stale refusal and last-good failure evidence', async ({ page, context, request }) => {
    const fixture = JSON.parse(await readFile(process.env.DUSKCUE_MANAGEMENT_FIXTURE!, 'utf8'));
    await context.addCookies([{ name: 'session', value: fixture.token, domain: 'localhost', path: '/', httpOnly: true, sameSite: 'Strict' }]);
    await page.addInitScript((user) => { localStorage.setItem('duskcue_user', JSON.stringify(user)); localStorage.setItem('duskcue_device_id', 'management-browser-qualification'); }, fixture.user);
    await page.route('**/api/v1/**', async (route) => { await new Promise((resolve) => setTimeout(resolve, 650)); await route.continue(); });
    await page.route('**/api/v1/events', (route) => route.fulfill({ contentType: 'text/event-stream', body: ': finite fixture\n\n' }));
    const errors: string[] = []; let starts = 0;
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('request', (entry) => { if (entry.method() === 'POST' && new URL(entry.url()).pathname.endsWith('/refresh')) starts += 1; });
    await page.goto('/settings/household');
    await page.getByText('Enroll or recover with an operator handoff', { exact: true }).click();
    await page.getByLabel('Operator handoff', { exact: true }).fill(fixture.handoff);
    await page.getByLabel('Existing account password', { exact: true }).fill(fixture.password);
    await page.getByRole('button', { name: 'Enable owner protection', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
    await page.goto('/settings/live-tv-control');
    await page.getByLabel('Allow manual imports', { exact: true }).check();
    await page.getByRole('button', { name: 'Save server refresh control', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Change saved.' })).toBeVisible();
    await page.goto('/settings/live-tv');
    await page.getByLabel('Choose a household', { exact: true }).selectOption(fixture.target);
    await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill(`${fixture.origin}/playlist?secret=private-paired-fixture`);
    await page.getByLabel('Dispatcharr XMLTV URL', { exact: true }).fill(`${fixture.origin}/guide?secret=private-paired-fixture`);
    await page.getByRole('button', { name: 'Create assignment', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeEnabled();
    await page.getByLabel('Household Live TV allowance', { exact: true }).check();
    await page.getByLabel('Allow the household owner to manage standard-profile access', { exact: true }).check();
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Approve destination', exact: true })).toBeEnabled();
    for (const feed of ['playlist', 'guide']) {
        await page.getByLabel('Feed', { exact: true }).selectOption(feed);
        await page.getByLabel('Expected feed origin', { exact: true }).fill(fixture.origin);
        await page.getByLabel('Approved IP addresses, one per line in connection order', { exact: true }).fill('127.0.0.1');
        await page.getByRole('button', { name: 'Approve destination', exact: true }).click();
        await expect(page.getByText(`${feed === 'playlist' ? 'Playlist' : 'Guide'}: 1 approved address choices`, { exact: true })).toBeVisible();
    }
    const start = async () => {
        await page.getByLabel('Playlist address choice', { exact: true }).selectOption('0');
        await page.getByLabel('Guide address choice', { exact: true }).selectOption('0');
        await page.getByRole('button', { name: 'Start manual refresh', exact: true }).click();
    };
    await start();
    await expect(page.getByRole('status').filter({ hasText: 'Latest refresh succeeded' })).toBeVisible({ timeout: 30000 });
    await expect(page.getByText('Channels: 1; guide channels: 1; programmes: 1.', { exact: true })).toBeVisible();
    expect(starts).toBe(1);
    await page.screenshot({ path: test.info().outputPath('real-paired-import.png') });
    await request.post(`${fixture.origin}/invalidate`);
    await start();
    await expect(page.getByRole('status').filter({ hasText: 'Latest refresh failed' })).toBeVisible({ timeout: 30000 });
    await expect(page.getByText('Channels: 1; guide channels: 1; programmes: 1.', { exact: true })).toBeVisible();
    await page.getByRole('button', { name: 'Check status', exact: true }).click();
    expect(starts).toBe(2);
    const resource = `/api/v1/live-tv/households/${fixture.target}/assignment`;
    const current = await context.request.get(resource); const etag = current.headers().etag;
    const session = await (await context.request.get('/api/v1/auth/session-context')).json();
    expect((await context.request.patch(resource, { headers: { 'If-Match': etag, 'X-Session-Context': session.context_id }, data: { operation: 'configure', enabled: true, max_live_streams: 3, allow_owner_profile_access: true, allow_owner_live_limit: false } })).status()).toBe(200);
    await page.getByLabel('Administrator Live TV ceiling', { exact: true }).fill('2');
    await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('session or resource changed');
    await page.getByRole('button', { name: 'Reload metadata', exact: true }).click();
    await expect(page.getByLabel('Administrator Live TV ceiling', { exact: true })).toHaveValue('3');
    expect(await page.evaluate(() => JSON.stringify(Object.entries(localStorage)))).not.toContain('private-paired-fixture');
    expect(errors).toEqual([]);
});
