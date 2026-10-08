// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

export function isNativeAppDocument(value) {
    try {
        const url = new URL(value);
        return ['http:', 'https:'].includes(url.protocol) && url.hostname === 'tauri.localhost' && !url.port && !url.username && !url.password;
    } catch { return false; }
}

export async function awaitNativeFirstRun(page, { timeoutMs = 30000, now = Date.now } = {}) {
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs <= 0) throw new Error('Native startup readiness requires a positive bounded deadline.');
    const startedAt = now();
    const deadline = startedAt + timeoutMs;
    const remaining = () => {
        const time = deadline - now();
        if (time <= 0) throw new Error('Native first-run document readiness exceeded its deadline.');
        return time;
    };
    await page.waitForURL((url) => isNativeAppDocument(url.href), { waitUntil: 'domcontentloaded', timeout: remaining() });
    await page.getByRole('heading', { name: 'Connect to your server', exact: true }).waitFor({ state: 'visible', timeout: remaining() });
    await page.getByLabel('Server URL', { exact: true }).waitFor({ state: 'visible', timeout: remaining() });
    await page.waitForFunction(() => document.readyState === 'complete' && typeof window.__TAURI_INTERNALS__?.invoke === 'function', undefined, { timeout: remaining() });
    if (!isNativeAppDocument(page.url())) throw new Error('The native app document changed before its first isolation check.');
    return { url: page.url(), committedNativeOrigin: true, visibleFirstRunServerGate: true, readyNativeBridge: true, elapsedMs: now() - startedAt };
}
