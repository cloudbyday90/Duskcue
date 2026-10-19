// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { openDirectBridge } from './direct-bridge.mjs';
import { prepareDirectPlayerBrowser } from './direct-browser-build.mjs';
import { startDirectGateway } from './direct-gateway.mjs';
import { runFormatBrowser } from './format-browser.mjs';
import { runSourceBrowser } from '../../clients/web/tests/native/qualification-source-browser.mjs';
import { runSourceRecoveryBrowser } from '../../clients/web/tests/native/qualification-source-recovery.mjs';

export async function prepareDirectBrowser(workspace, directory) {
    return prepareDirectPlayerBrowser(workspace, directory);
}

export async function runDirectBrowser({ workspace, directory, context, runtime, resourceId, execution, ownership, assets, abortSignal, nativeManifest, nativeRecovery = false, formatCorpus = false, sourceCapabilities = false }) {
    let bridge;
    let gateway;
    let nativeGateway;
    let browser;
    let browserContext;
    let runtimeDone = false;
    const observed = execution.finally(() => { runtimeDone = true; });
    observed.catch(() => {});
    const requests = [];
    const report = { resourceId, passed: false, trials: [], fullPlayer: true, productionRouter: true, transport: 'Actual Player and production Axum router through owned loopback/Docker-exec transport; no mocked API responses.' };
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
        assert.equal(ready.nativeRecovery === true, nativeRecovery);
        assert.equal(Array.isArray(ready.formats), formatCorpus);
        assert.equal(ready.sourceCapabilities === true, sourceCapabilities);
        assert.match(ready.token, /^[A-Za-z0-9_-]{32,256}$/);
        assert.ok(Array.isArray(ready.candidates) && ready.candidates.length > 0 && ready.candidates.length <= 4);
        const denied = await bridge.request({ operation: 'fetch', path: `/api/v1/stream/${ready.candidates[0].file}`, method: 'GET', headers: {}, body: '' });
        assert.equal(denied.status, 401);
        report.unauthenticatedMediaStatus = denied.status;
        gateway = await startDirectGateway({ bridge, assets, requests });
        const origin = gateway.origin;
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
                assert.equal(played.recoveryNoticeVisible, true);
                assert.equal(played.recoveryNotice, 'A recovery stream is ready at reduced playback quality.');
                assert.equal(played.selectedAudioIndex, 2);
                const region = page.getByRole('region', { name: 'Media player', exact: true });
                const box = await region.boundingBox();
                assert.ok(box && box.width >= 320 && box.height >= 180, 'The actual Player must occupy a visible viewport.');
                await region.hover();
                await region.getByRole('button', { name: 'Pause', exact: true }).click();
                const seek = region.getByRole('slider', { name: 'Seek', exact: true });
                await seek.focus();
                await page.keyboard.press('Home');
                for (let step = 0; step < 50; step += 1) await page.keyboard.press('ArrowRight');
                await page.keyboard.press('Tab');
                await page.waitForFunction((old) => { const s = window.directFixture.snapshot(); return !s.seeking && s.transcodeId && s.transcodeId !== old && s.offsetMs === 5000 && s.paused; }, played.transcodeId, { timeout: 15000 });
                await region.getByRole('button', { name: 'Play', exact: true }).click();
                await page.waitForFunction(() => { const s = window.directFixture.snapshot(); const current = s.generations.find((g) => g.id === s.transcodeId); return current?.frames >= 5 && s.hlsElapsedMs >= 500 && s.peakDb > -60 && Math.abs(s.peakHz - 880) < 20; }, undefined, { timeout: 15000 });
                await region.getByRole('button', { name: 'Pause', exact: true }).click();
                const sought = await page.evaluate(() => window.directFixture.snapshot());
                assert.equal(sought.sessionId, started.sessionId);
                assert.equal(sought.recovery.attempts, 1);
                assert.equal(sought.selectedAudioIndex, 2);
                assert.equal(sought.paused, true);
                assert.ok(Number.isFinite(sought.firstFrameMediaTime) && sought.firstFrameMediaTime >= 0 && sought.firstFrameMediaTime <= 0.25);
                assert.ok(sought.frameMediaTime > sought.firstFrameMediaTime);
                const verified = await bridge.request({ operation: 'verify', report: { resourceId, phase: 'browser', sessionId: started.sessionId, transcodeId: sought.transcodeId, previousTranscodeId: played.transcodeId, positionMs: 5000, paused: sought.paused, attempts: 1 } });
                assert.equal(verified.verified.passed, true);
                assert.equal(verified.verified.resourceId, resourceId);
                report.postSeek = { ...sought, backend: verified.verified.backend };
                report.recoveryNotice = played.recoveryNotice;
                report.recoveryNoticeVisible = played.recoveryNoticeVisible;
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
                await region.getByRole('button', { name: 'Close player', exact: true }).click();
                await page.waitForFunction(() => { const s = window.directFixture.snapshot(); return s.closed && !s.sessionId; }, undefined, { timeout: 10000 });
                report.closedThroughUi = true;
                await page.evaluate(() => window.directFixture.stop());
                break;
            } catch (error) {
                report.passed = false;
                await page.screenshot({ path: join(directory, `direct-browser-failed-${candidate.variant}.png`) }).catch(() => {});
                report.trials.push({ variant: candidate.variant, result: error.message.slice(0, 2048), snapshot: await page.evaluate(() => window.directFixture.snapshot()) });
                await page.evaluate(() => window.directFixture.stop());
                if (report.postSeek) throw error;
            }
        }
        report.authenticatedMediaRequests = requests.filter((request) => /\/(stream|transcode)\//.test(request.path));
        assert.ok(report.authenticatedMediaRequests.some((request) => request.path.endsWith('.m3u8') && request.status === 200));
        assert.ok(report.authenticatedMediaRequests.some((request) => request.path.endsWith('.m4s') && request.status === 200));
        assert.ok(report.authenticatedMediaRequests.every((request) => request.cookie && !request.bearer));
        assert.deepEqual(pageErrors, []);
        assert.deepEqual(gateway.errors, []);
        assert.equal(report.passed, true, 'No real native decode failure recovered to decoded authenticated HLS.');
        const verifyFormat = async (evidence) => {
            const verified = await bridge.request({ operation: 'verify-format', report: { ...evidence, resourceId } });
            assert.equal(verified.verified.passed, true);
            assert.equal(verified.verified.resourceId, resourceId);
            assert.equal(verified.verified.phase, evidence.phase);
            return { ...verified.verified.backend, passed: true };
        };
        const verifySource = async (evidence) => {
            const verified = await bridge.request({ operation: 'verify-source', report: { ...evidence, resourceId } });
            assert.equal(verified.verified.passed, true);
            assert.equal(verified.verified.resourceId, resourceId);
            assert.equal(verified.verified.phase, evidence.phase);
            return { ...verified.verified.backend, passed: true };
        };
        if (ready.formats) {
            report.formats = await runFormatBrowser({ page, ready, origin, directory, verifyFormat, abortSignal });
            assert.deepEqual(pageErrors, []);
            assert.deepEqual(gateway.errors, []);
        }
        if (sourceCapabilities) {
            report.sourceCapabilities = await runSourceBrowser({ page, ready, origin, directory, verifySource, abortSignal });
            report.sourceRecovery = await runSourceRecoveryBrowser({ page, ready, origin, directory, verifySource, abortSignal });
            assert.deepEqual(pageErrors, []);
            assert.deepEqual(gateway.errors, []);
        }
        await browserContext.close(); browserContext = null;
        await browser.close(); browser = null;
        if (nativeManifest) {
            const url = new URL(nativeManifest.origins[0]);
            nativeGateway = await startDirectGateway({ bridge, host: url.hostname, port: Number(url.port) });
            const { runNativeLive } = await import(pathToFileURL(join(workspace, 'clients/web/tests/native/qualification-live.mjs')));
            report.native = await runNativeLive({ manifest: nativeManifest, ready, gatewayOrigin: nativeGateway.origin, directory, abortSignal, verifyFormat, verifySource,
                async armRecovery(evidence) {
                    assert.equal(nativeRecovery, true);
                    const armed = await bridge.request({ operation: 'arm-native-stall', report: { ...evidence, resourceId } });
                    return armed.armed;
                },
                async verifyPlayback(evidence) {
                    const verified = await bridge.request({ operation: 'verify', report: { ...evidence, resourceId } });
                    assert.equal(verified.verified.passed, true);
                    assert.equal(verified.verified.resourceId, resourceId);
                    return { ...verified.verified.backend, passed: true };
                } });
            assert.deepEqual(nativeGateway.errors, []);
        }
        await bridge.request({ operation: 'complete', report });
    } catch (error) {
        report.passed = false; report.error = error.message;
        if (bridge && !runtimeDone) await bridge.request({ operation: 'complete', report }).catch(() => {});
        throw error;
    } finally {
        const failures = [];
        for (const [name, close] of [
            ['browserContext', () => browserContext?.close()], ['browser', () => browser?.close()],
            ['nativeGateway', () => nativeGateway?.close()], ['gateway', () => gateway?.close()], ['bridge', () => bridge?.close()],
        ]) {
            try { await close(); } catch { failures.push(name); }
        }
        report.cleanup = { passed: failures.length === 0, failures };
        await writeFile(join(directory, 'direct-browser.json'), `${JSON.stringify(report, null, 2)}\n`);
        if (failures.length) throw new Error(`Owned live-client cleanup failed: ${failures.join(', ')}.`);
    }
    return { execution: await observed, browser: report };
}
