/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';
import { keyboardReach } from './qualification-focus.mjs';
import { captureNativeArtifact } from './qualification-capture.mjs';
import { managementIds, fixtureAssignment, createManagementFixture } from '../fixtures/live-tv-management.js';
import { join } from 'node:path';

export async function exerciseNativeManagement(page, api, manifest, inventory, label = 'primary') {
    const evidence = { focus: {}, contrast: inventory.contrast, zoom: null, bearerRouted: false, exactJson: false, setup: false, owner: false, confirmation: false };
    const appOrigin = new URL(page.url()).origin; const origin = manifest.origins[label === 'primary' ? 0 : 1];
    const role = api.scenario.user.role;
    api.scenario.user.role = 'owner'; api.scenario.management = createManagementFixture();
    api.scenario.household.grant = new Date(Date.now() + 300000).toISOString();
    try {
        await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
        evidence.exactJson = await page.evaluate(() => {
            let text; JSON.parse('{"revision":9007199254740993}', (key, value, context) => { if (key === 'revision') text = context?.source; return value; }); return text === '9007199254740993';
        });
        expect(evidence.exactJson).toBe(true);
        await page.goto(`${appOrigin}/settings/live-tv-control`);
        await page.getByLabel('Allow manual imports', { exact: true }).check();
        await page.getByRole('button', { name: 'Save server refresh control', exact: true }).click();
        await expect.poll(() => api.scenario.management.control.enabled).toBe(true);
        await page.goto(`${appOrigin}/settings/live-tv`);
        await page.getByLabel('Choose a household', { exact: true }).selectOption(managementIds.target);
        await page.getByLabel('Dispatcharr M3U URL', { exact: true }).fill('http://dispatcharr.invalid/playlist?native=private');
        await page.getByLabel('Dispatcharr XMLTV URL', { exact: true }).fill('http://dispatcharr.invalid/guide?native=private');
        await page.getByRole('button', { name: 'Create assignment', exact: true }).click();
        await expect(page.getByRole('button', { name: 'Save administrator policy', exact: true })).toBeEnabled();
        await page.getByLabel('Household Live TV allowance', { exact: true }).check();
        await page.getByRole('button', { name: 'Save administrator policy', exact: true }).click();
        await expect(page.getByRole('button', { name: 'Approve destination', exact: true })).toBeEnabled();
        for (const feed of ['playlist', 'guide']) {
            await page.getByLabel('Feed', { exact: true }).selectOption(feed);
            await page.getByLabel('Expected feed origin', { exact: true }).fill('http://dispatcharr.invalid');
            await page.getByLabel('Approved IP addresses, one per line in connection order', { exact: true }).fill('127.0.0.1');
            await page.getByRole('button', { name: 'Approve destination', exact: true }).click();
            await expect(page.getByText(`${feed === 'playlist' ? 'Playlist' : 'Guide'}: 1 approved address choices`, { exact: true })).toBeVisible();
        }
        await page.getByLabel('Playlist address choice', { exact: true }).selectOption('0');
        await page.getByLabel('Guide address choice', { exact: true }).selectOption('0');
        await page.getByRole('button', { name: 'Start manual refresh', exact: true }).click();
        await expect(page.getByRole('status').filter({ hasText: 'Latest refresh succeeded' })).toBeVisible();
        expect(api.scenario.management.starts).toBe(1); evidence.setup = true;
        await captureNativeArtifact(page, manifest, inventory, `native-management-${label}`, false);
        await page.getByRole('button', { name: 'Revoke destination', exact: true }).click();
        const dialog = page.getByRole('dialog', { name: 'Review action', exact: true });
        await expect(dialog.getByRole('button', { name: 'Keep editing', exact: true })).toBeFocused();
        await page.keyboard.press('Escape');
        expect(api.scenario.management.writes.some((entry) => entry.operation === 'revoke_destination')).toBe(false); evidence.confirmation = true;
        api.scenario.management.assignments.set(api.scenario.user.id, fixtureAssignment(api.scenario.user.id, { enabled: true, allow_owner_profile_access: true, allow_owner_live_limit: true }));
        await page.goto(`${appOrigin}/settings/household-live-tv`);
        const standard = api.scenario.profiles.find((p) => p.profile_type === 'standard');
        await page.getByRole('checkbox', { name: `Live TV access for ${standard.name}`, exact: true }).click();
        await expect.poll(() => api.scenario.management.profiles.get(standard.id)).toBe(true);
        await page.getByRole('textbox', { name: 'Additional household Live TV ceiling', exact: true }).fill('2');
        await page.getByRole('button', { name: 'Save household ceiling', exact: true }).click();
        await expect.poll(() => api.scenario.management.assignments.get(api.scenario.user.id).owner_max_live_streams).toBe(2); evidence.owner = true;
        const baseline = await page.evaluate(() => ({ width: innerWidth, dpr: devicePixelRatio }));
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 4 });
        await page.goto(`${appOrigin}/ar/settings/live-tv`);
        await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
        await expect(page.locator('.management')).toHaveAttribute('lang', 'en');
        await page.getByLabel('Choose a household', { exact: true }).selectOption(managementIds.target);
        const input = page.getByLabel('Dispatcharr M3U URL', { exact: true });
        await keyboardReach(page, input, `management-${label}-400-source`, evidence);
        await captureNativeArtifact(page, manifest, inventory, `native-management-${label}-400-rtl`, false);
        evidence.zoom = await page.evaluate(() => ({ width: innerWidth, dpr: devicePixelRatio, clientWidth: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth }));
        expect(evidence.zoom.width).toBeLessThanOrEqual(320); expect(evidence.zoom.scrollWidth).toBeLessThanOrEqual(evidence.zoom.clientWidth + 1);
        expect(evidence.zoom.width).toBeCloseTo(baseline.width / 4, 0); expect(evidence.zoom.dpr / baseline.dpr).toBeCloseTo(4, 2);
        const reads = api.transportRequests.filter((r) => r.origin === origin && (r.path.startsWith('/live-tv/') || r.path.startsWith('/household-management/live-tv')));
        expect(reads.length > 0 && reads.every((r) => r.authenticated && r.originAllowed)).toBe(true); evidence.bearerRouted = true;
    } catch (error) {
        await page.screenshot({ path: join(manifest.directory, `management-${label}-failure.png`), fullPage: true }).catch(() => {});
        error.message += `; management alerts: ${JSON.stringify(await page.getByRole('alert').allTextContents())}`;
        throw error;
    } finally {
        api.scenario.user.role = role;
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 1 });
        await page.goto(`${appOrigin}/dashboard`);
    }
    return evidence;
}
