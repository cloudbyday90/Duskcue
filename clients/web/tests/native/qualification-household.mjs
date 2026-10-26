/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';
import { keyboardReach } from './qualification-focus.mjs';
import { captureNativeArtifact } from './qualification-capture.mjs';

export async function exerciseNativeHousehold(page, api, manifest, inventory, label = 'primary') {
    const evidence = { focus: {}, contrast: inventory.contrast, rotatedKeyring: false, badProofPreservesViewing: false, nativePasskeysAbsent: false, zoom: null };
    const appOrigin = new URL(page.url()).origin;
    const origin = label === 'primary' ? manifest.origins[0] : manifest.origins[1];
    const before = api.tokens.get(origin);
    api.scenario.household.enabled = false; api.scenario.household.trustedPassword = false; api.scenario.household.grant = null;
    api.scenario.household.handoff = `owner-handoff-${label}`;
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('link', { name: 'Settings', exact: true }).click();
    await page.getByRole('link', { name: 'Household management', exact: true }).click();
    await expect(page.getByText('Owner-trusted passkeys: 0', { exact: true })).toBeVisible();
    await page.getByText('Enroll or recover with an operator handoff', { exact: true }).click();
    const handoff = page.locator('#owner-handoff');
    await keyboardReach(page, handoff, `household-${label}-handoff`, evidence);
    await handoff.fill(api.scenario.household.handoff);
    await page.locator('#owner-enrollment-password').fill(api.scenario.household.password);
    await page.getByRole('button', { name: 'Enable owner protection', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
    const saved = await invokeNative(page, 'read_session_token', { req: { server_origin: origin } });
    expect(saved === api.tokens.get(origin) && saved !== before).toBe(true);
    evidence.rotatedKeyring = true;
    await expect(page.getByRole('button', { name: 'Verify owner passkey', exact: true })).toHaveCount(0);
    await expect(page.locator('#owner-method option')).toHaveCount(1);
    await expect(page.getByText(/Use the server-hosted browser for passkey enrollment/)).toBeVisible();
    evidence.nativePasskeysAbsent = true;
    await page.getByRole('button', { name: 'Lock management', exact: true }).click();
    await expect(page.getByRole('status').filter({ hasText: 'Management is locked' })).toBeVisible();
    await page.locator('#owner-password').fill('incorrect-owner-password');
    await page.getByRole('button', { name: 'Verify owner password', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('viewing session has not been signed out');
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toBeVisible();
    expect(await invokeNative(page, 'read_session_token', { req: { server_origin: origin } }) === saved).toBe(true);
    evidence.badProofPreservesViewing = true;
    await captureNativeArtifact(page, manifest, inventory, `native-household-${label}`, false);
    const baseline = await page.evaluate(() => ({ width: innerWidth, dpr: devicePixelRatio }));
    try {
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 4 });
        await page.goto(`${appOrigin}/ar/settings/household`);
        await expect(page.locator('html')).toHaveAttribute('dir', 'rtl');
        await expect(page.locator('.household')).toHaveAttribute('lang', 'en');
        const password = page.locator('#owner-password');
        await expect(password).toHaveAttribute('autocomplete', 'current-password');
        await keyboardReach(page, password, `household-${label}-400-password`, evidence);
        await password.fill(api.scenario.household.password);
        await keyboardReach(page, page.getByRole('button', { name: 'Verify owner password', exact: true }), `household-${label}-400-submit`, evidence);
        await captureNativeArtifact(page, manifest, inventory, `native-household-${label}-400-rtl`, false);
        await page.keyboard.press('Enter');
        await expect(page.getByRole('status').filter({ hasText: 'Management verified for this session' })).toBeVisible();
        evidence.zoom = await page.evaluate(() => ({ width: innerWidth, dpr: devicePixelRatio, clientWidth: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth }));
        expect(evidence.zoom.width).toBeLessThanOrEqual(320);
        expect(evidence.zoom.scrollWidth).toBeLessThanOrEqual(evidence.zoom.clientWidth + 1);
        expect(evidence.zoom.width).toBeCloseTo(baseline.width / 4, 0);
        expect(evidence.zoom.dpr / baseline.dpr).toBeCloseTo(4, 2);
    } finally {
        await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value: 1 });
        await page.goto(`${appOrigin}/dashboard`);
    }
    const reads = api.transportRequests.filter((entry) => entry.path.startsWith('/household-management/') && entry.origin === origin);
    expect(reads.length > 0 && reads.every((entry) => entry.authenticated && entry.originAllowed)).toBe(true);
    return evidence;
}
