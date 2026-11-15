// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';
import { join } from 'node:path';
import { lstat } from 'node:fs/promises';
import { BrowserOwner } from './browser-owner.mjs';
import { CACHE_VALUE } from './fixture.mjs';

export async function runBrowserCases(inputs, origin, holder, owners, signal, fail) {
    const { chromium } = await import(pathToFileURL(join(inputs.web, 'node_modules/@playwright/test/index.mjs')).href);
    const executablePath = chromium.executablePath();
    const executable = await lstat(executablePath);
    assert.ok(executable.isFile() && !executable.isSymbolicLink(), 'Install the existing locked Chromium separately; no download is permitted here.');
    const owner = new BrowserOwner(chromium, { signal, executablePath, output: (chunk) => owners.acceptOutput(chunk), fail });
    holder.current = owner;
    let rows;
    try {
        const context = await owner.open();
        let foreignRequests = 0;
        await context.route('**/*', async (route) => {
            const url = new URL(route.request().url());
            if (url.origin === origin) return route.continue();
            foreignRequests++;
            await route.fulfill({ status: 200, contentType: 'application/javascript', body: 'globalThis.__duskcueOriginExecuted=1; export default {};' });
        });
        const page = await context.newPage();
        page.setDefaultTimeout(5000);
        page.setDefaultNavigationTimeout(10000);
        let dialogs = 0;
        page.on('dialog', async (dialog) => { dialogs++; await dialog.dismiss(); });
        const cacheResponse = await page.goto(`${origin}/cache`);
        assert.equal(cacheResponse.status(), 200);
        assert.equal(await page.locator('h1').textContent(), 'Cache fixture');
        assert.equal(await page.locator('meta[http-equiv="cache-control"]').count(), 1);
        assert.equal(await page.locator('meta[http-equiv="cache-control"]').getAttribute('content'), CACHE_VALUE);
        assert.equal(await page.evaluate(() => globalThis.__duskcueInlineControl), 1);
        assert.equal(await page.evaluate(() => globalThis.__duskcueCacheExecuted === undefined), true);
        assert.equal(dialogs, 0);
        let healthyRouteRequests = 0;
        let hostileRouteRequests = 0;
        page.on('request', (request) => {
            const url = new URL(request.url());
            if (url.origin !== origin || !url.pathname.endsWith('/__route.js')) return;
            if (url.pathname.startsWith('//foreign.invalid/')) hostileRouteRequests++;
            else healthyRouteRequests++;
        });
        await page.goto(`${origin}/nav/one`);
        await page.waitForFunction(() => globalThis.__duskcueHydrated === true && typeof globalThis.__duskcueGoto === 'function');
        await page.evaluate(() => globalThis.__duskcueGoto('/nav/two'));
        assert.equal(await page.locator('h1').textContent(), 'Navigation two');
        assert.ok(healthyRouteRequests >= 1, 'The positive control did not exercise server route resolution.');
        await Promise.all([
            page.waitForURL((url) => url.origin === origin && url.pathname === '//foreign.invalid/x', { waitUntil: 'domcontentloaded' }),
            page.evaluate(() => { setTimeout(() => { globalThis.__duskcueGoto('/nav/..//foreign.invalid/x').catch(() => {}); }, 0); }),
        ]);
        assert.ok(hostileRouteRequests >= 1, 'The hostile path did not exercise the same-origin resolution request.');
        assert.equal(new URL(page.url()).origin, origin);
        assert.equal(await page.evaluate(() => globalThis.__duskcueOriginExecuted === undefined), true);
        assert.equal(foreignRequests, 0);
        owner.check();
        rows = [{ id: 'cache-browser', status: 'passed', inlineControl: 1, markerAbsent: true, dialogs },
            { id: 'client-origin', status: 'passed', hydrated: true, healthyRouteRequests, hostileRouteRequests, foreignRequests, markerAbsent: true }];
    } finally { await owner.close(); }
    return rows.map((row) => ({ ...row, cleanupConfirmed: owner.cleanupConfirmed }));
}
