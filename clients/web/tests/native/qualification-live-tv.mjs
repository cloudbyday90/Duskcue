/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';
import { keyboardReach } from './qualification-focus.mjs';
import { captureNativeArtifact } from './qualification-capture.mjs';

export async function exerciseNativeLiveTv(page, api, manifest, inventory, label = 'primary') {
    const evidence = { focus: {}, contrast: inventory.contrast, selectedOrigin: null, bearerRouted: false, exactEvidence: false, recovery: false, zoom: null };
    const appOrigin = new URL(page.url()).origin;
    const before = api.transportRequests.length;
    const channel = page.locator('.channels button').first();
    await invokeNative(page, 'plugin:event|emit', { event: 'duskcue://navigate', payload: { route: '/live-tv', source: 'qualification' } });
    await expect.poll(() => new URL(page.url()).pathname).toBe('/live-tv');
    await expect(channel).toContainText('World & Culture');
    await expect(page.locator('.programme')).toHaveCount(0);
    await keyboardReach(page, channel, `live-tv-${label}-channel`, evidence);
    await page.keyboard.press('Enter');
    await expect(page.locator('#live-tv-guide')).toBeFocused();
    await expect(page.locator('.programme')).toHaveCount(20);
    const summary = page.locator('.programme').first().getByText('Supplied timing details', { exact: true });
    await keyboardReach(page, summary, `live-tv-${label}-raw-details`, evidence);
    await page.keyboard.press('Enter');
    await expect(page.locator('.programme').first()).toContainText('.123456Z');
    evidence.exactEvidence = true;
    await captureNativeArtifact(page, manifest, inventory, `native-live-tv-${label}`, false);
    api.scenario.liveTv.status = 409;
    await page.getByRole('navigation', { name: 'Programme pages', exact: true }).getByRole('button', { name: 'Next', exact: true }).click();
    await expect(page.locator('.programme, .channels')).toHaveCount(0);
    await expect(page.getByRole('heading', { name: 'Live TV', exact: true })).toBeFocused();
    api.scenario.liveTv.status = 200;
    await keyboardReach(page, page.getByRole('button', { name: 'Reload Live TV', exact: true }), `live-tv-${label}-reload`, evidence);
    await page.keyboard.press('Enter');
    await expect(channel).toContainText('World & Culture');
    evidence.recovery = true;
    const baseline = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio }));
    try {
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 4 });
        await page.goto(`${appOrigin}/ar/live-tv`);
        await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
        await expect(page.locator('.live-tv')).toHaveAttribute('lang', 'en');
        await expect(channel).toContainText('World & Culture');
        await expect.poll(() => page.evaluate(() => innerWidth)).toBeLessThanOrEqual(320);
        await keyboardReach(page, channel, `live-tv-${label}-400-channel`, evidence);
        await page.keyboard.press('Enter');
        await expect(page.locator('#live-tv-guide')).toBeFocused();
        const incomplete = page.getByRole('button', { name: 'Incomplete times', exact: true });
        await keyboardReach(page, incomplete, `live-tv-${label}-400-view`, evidence);
        await page.keyboard.press('Enter');
        await expect(page.locator('.programme')).toHaveCount(3);
        const titles = page.locator('.programme').first().getByText('All supplied titles', { exact: true });
        await keyboardReach(page, titles, `live-tv-${label}-400-disclosure`, evidence);
        await page.keyboard.press('Enter');
        await expect(page.locator('.programme').first().locator('details[open]').getByText('نشرة المساء', { exact: true })).toBeVisible();
        evidence.zoom = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio, clientWidth: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth }));
        expect(evidence.zoom.scrollWidth).toBeLessThanOrEqual(evidence.zoom.clientWidth + 1);
        expect(evidence.zoom.width).toBeCloseTo(baseline.width / 4, 0);
        expect(evidence.zoom.dpr / baseline.dpr).toBeCloseTo(4, 2);
        await captureNativeArtifact(page, manifest, inventory, `native-live-tv-${label}-400-rtl`, false);
    } finally {
        api.scenario.liveTv.status = 200;
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 1 });
        await page.goto(`${appOrigin}/dashboard`);
    }
    await expect.poll(() => api.transportRequests.slice(before).filter((entry) => entry.path.startsWith('/live-tv/catalog/')).length).toBeGreaterThan(0);
    const reads = api.transportRequests.slice(before).filter((entry) => entry.path.startsWith('/live-tv/catalog/'));
    expect(reads.every((entry) => entry.authenticated && entry.originAllowed && entry.origin === reads[0].origin)).toBe(true);
    evidence.selectedOrigin = reads[0].origin;
    evidence.bearerRouted = true;
    return evidence;
}
