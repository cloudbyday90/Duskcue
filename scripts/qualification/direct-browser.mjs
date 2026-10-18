// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { openDirectBridge } from './direct-bridge.mjs';

export async function prepareDirectBrowser(workspace, directory) {
    const { build } = await import(pathToFileURL(join(workspace, 'clients/web/node_modules/vite/dist/node/index.js')));
    const output = join(directory, 'browser');
    await mkdir(output);
    await build({ configFile: false, root: join(workspace, 'clients/web'), logLevel: 'warn',
        build: { outDir: output, emptyOutDir: false, minify: false, target: 'es2022', assetsDir: '.',
            rollupOptions: { input: join(workspace, 'clients/web/tests/fixtures/direct-live-entry.mjs'), output: { entryFileNames: 'direct.js', chunkFileNames: '[name]-[hash].js' } } } });
    return output;
}

export async function runDirectBrowser({ workspace, directory, context, runtime, resourceId, execution, ownership, assets, abortSignal }) {
    let bridge;
    let server;
    let browser;
    let browserContext;
    let runtimeDone = false;
    const observed = execution.finally(() => { runtimeDone = true; });
    observed.catch(() => {});
    const requests = [];
    const report = { resourceId, passed: false, trials: [], transport: 'Isolated loopback HTTP through owned Docker-exec gateway; real Axum authentication and FFmpeg, no mocked API responses.' };
    try {
        const deadline = Date.now() + 250000;
        while (!(await ownership.inspect(runtime)).state.running) {
            if (runtimeDone || Date.now() > deadline) throw new Error('Direct fixture did not start.');
            await new Promise((resolve) => setTimeout(resolve, 100));
        }
        bridge = openDirectBridge(context, runtime, abortSignal);
        let ready;
        while (!ready) {
            if (runtimeDone || Date.now() > deadline) throw new Error('Real direct fixture never became ready.');
            ready = (await bridge.request({ operation: 'ready' })).ready;
            if (!ready) await new Promise((resolve) => setTimeout(resolve, 500));
        }
        assert.equal(ready.resourceId, resourceId);
        assert.match(ready.token, /^[A-Za-z0-9_-]{32,256}$/);
        assert.ok(Array.isArray(ready.candidates) && ready.candidates.length > 0 && ready.candidates.length <= 4);
        const denied = await bridge.request({ operation: 'fetch', path: `/api/v1/stream/${ready.candidates[0].file}`, method: 'GET', headers: {}, body: '' });
        assert.equal(denied.status, 401);
        report.unauthenticatedMediaStatus = denied.status;
        const html = '<!doctype html><html lang="en"><title>Owned direct recovery qualification</title><button>Enable fixture audio</button><video controls playsinline></video><script type="module" src="/direct.js"></script></html>';
        server = createServer(async (request, response) => {
            try {
                if (request.method === 'GET' && request.url === '/') {
                    response.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store', 'content-security-policy': "default-src 'none'; script-src 'self'; media-src 'self' blob:; connect-src 'self'; worker-src blob:" });
                    response.end(html); return;
                }
                if (request.method === 'GET' && /^\/[A-Za-z0-9_.-]+\.js$/.test(request.url || '')) {
                    const body = await readFile(join(assets, request.url.slice(1)));
                    response.writeHead(200, { 'content-type': 'text/javascript', 'cache-control': 'no-store' }); response.end(body); return;
                }
                if (!['GET', 'POST'].includes(request.method) || !request.url?.startsWith('/api/v1/')) throw new Error('Fixture route is unavailable.');
                const chunks = []; let bytes = 0;
                for await (const chunk of request) { bytes += chunk.length; if (bytes > 65536) throw new Error('Fixture request is too large.'); chunks.push(chunk); }
                const headers = Object.fromEntries(Object.entries(request.headers).filter(([key]) => ['cookie', 'content-type', 'range', 'authorization'].includes(key)));
                const upstream = await bridge.request({ operation: 'fetch', path: request.url, method: request.method, headers, body: Buffer.concat(chunks).toString('utf8') });
                requests.push({ method: request.method, path: request.url, status: upstream.status, cookie: typeof headers.cookie === 'string', bearer: typeof headers.authorization === 'string' });
                if (requests.length > 1000) throw new Error('Fixture request evidence exceeded its bound.');
                response.writeHead(upstream.status, upstream.headers); response.end(Buffer.from(upstream.body, 'base64'));
            } catch { response.writeHead(502); response.end('Owned fixture request failed.'); }
        });
        await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
        const origin = `http://127.0.0.1:${server.address().port}`;
        const { chromium } = await import(pathToFileURL(join(workspace, 'clients/web/node_modules/@playwright/test/index.mjs')));
        browser = await chromium.launch({ channel: 'chrome', headless: true });
        browserContext = await browser.newContext({ serviceWorkers: 'block' });
        await browserContext.addCookies([{ name: 'session', value: ready.token, url: origin, httpOnly: true, sameSite: 'Strict' }]);
        const page = await browserContext.newPage();
        const pageErrors = [];
        page.on('pageerror', (error) => pageErrors.push(error.message.slice(0, 512)));
        await page.goto(origin, { timeout: 15000 });
        await page.waitForFunction(() => Boolean(window.directFixture), undefined, { timeout: 15000 });
        await page.getByRole('button', { name: 'Enable fixture audio' }).click();
        report.browser = browser.version();
        for (const candidate of ready.candidates) {
            const started = await page.evaluate(({ fixture, candidate }) => window.directFixture.start(fixture, candidate), { fixture: ready, candidate });
            assert.equal(started.decision, 'direct_play');
            try {
                await page.waitForFunction(() => window.directFixture.snapshot().errors.length > 0 || document.querySelector('video').ended, undefined, { timeout: 12000 });
                const failed = await page.evaluate(() => window.directFixture.snapshot());
                const decoded = failed.errors.find((error) => error.code === 3 && error.trusted);
                if (!decoded || failed.directFrames < 1) { report.trials.push({ variant: candidate.variant, result: 'Native decode failure after presented frames not observed', snapshot: failed }); await page.evaluate(() => window.directFixture.stop()); continue; }
                await page.waitForFunction(() => { const s = window.directFixture.snapshot(); return s.recovery?.status === 'recovered' && s.hlsFrames >= 5 && s.ready >= 2 && s.audioRunning && s.hlsElapsedMs >= 500 && s.peakDb > -60 && Math.abs(s.peakHz - 880) < 20; }, undefined, { timeout: 20000 });
                const played = await page.evaluate(() => window.directFixture.snapshot());
                assert.equal(played.sessionId, started.sessionId);
                assert.equal(played.decision, 'transcode');
                assert.equal(played.recovery.attempts, 1);
                assert.equal(played.recovery.quality_reduced, true);
                assert.equal(played.recovery.skipped_ms, 0);
                assert.ok(Math.abs(played.offsetMs - decoded.position * 1000) <= 100);
                report.passed = true;
                report.variant = candidate.variant;
                report.sourceSha256 = candidate.sha256;
                report.nativeError = decoded;
                report.directFrames = failed.directFrames;
                report.hlsFrames = played.hlsFrames;
                report.selectedAudioHz = played.peakHz;
                report.selectedAudioDb = played.peakDb;
                report.hlsElapsedMs = played.hlsElapsedMs;
                report.recovery = played.recovery;
                report.offsetMs = played.offsetMs;
                await page.screenshot({ path: join(directory, 'direct-browser.png') });
                await page.evaluate(() => window.directFixture.stop());
                break;
            } catch (error) {
                report.trials.push({ variant: candidate.variant, result: error.message.slice(0, 512), snapshot: await page.evaluate(() => window.directFixture.snapshot()) });
                await page.evaluate(() => window.directFixture.stop());
            }
        }
        report.authenticatedMediaRequests = requests.filter((request) => /\/(stream|transcode)\//.test(request.path));
        assert.ok(report.authenticatedMediaRequests.some((request) => request.path.endsWith('.m3u8') && request.status === 200));
        assert.ok(report.authenticatedMediaRequests.some((request) => request.path.endsWith('.m4s') && request.status === 200));
        assert.ok(report.authenticatedMediaRequests.every((request) => request.cookie && !request.bearer));
        assert.deepEqual(pageErrors, []);
        assert.equal(report.passed, true, 'No real native decode failure recovered to decoded authenticated HLS.');
        await bridge.request({ operation: 'complete', report });
    } catch (error) {
        report.passed = false; report.error = error.message;
        if (bridge && !runtimeDone) await bridge.request({ operation: 'complete', report }).catch(() => {});
        throw error;
    } finally {
        await browserContext?.close();
        await browser?.close();
        if (server) { server.closeAllConnections(); await new Promise((resolve) => server.close(resolve)); }
        await bridge?.close();
        await writeFile(join(directory, 'direct-browser.json'), `${JSON.stringify(report, null, 2)}\n`);
    }
    return { execution: await observed, browser: report };
}
