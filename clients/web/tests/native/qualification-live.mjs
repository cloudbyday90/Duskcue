// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { readQualification } from './qualification-config.mjs';
import { launchQualification, stopQualification } from './qualification-runtime.mjs';
import { assertNativeIsolation } from './qualification-connection.mjs';
import { awaitNativeFirstRun } from './qualification-startup.mjs';
import { assertNativeLiveMedia, nativeLiveFailure, nativeLiveScenario } from './qualification-live-policy.mjs';
import { observeNativeLive } from './qualification-live-observation.mjs';
import { clearNativeLiveCredential, connectNativeLive } from './qualification-live-session.mjs';
import { observeNativeAudio, releaseNativeAudio } from './qualification-live-media.mjs';
import { recoverPausedNative } from './qualification-live-recovery.mjs';

async function decoded(video) {
    await expect.poll(() => video.evaluate((element) => ({ ready: element.readyState >= 2, width: element.videoWidth > 0,
        frames: (element.getVideoPlaybackQuality?.().totalVideoFrames || 0) > 0, advancing: element.currentTime > 0.2 })), { timeout: 20000 })
        .toEqual({ ready: true, width: true, frames: true, advancing: true });
    const first = await video.evaluate((element) => ({ time: element.currentTime, frames: element.getVideoPlaybackQuality().totalVideoFrames }));
    await expect.poll(() => video.evaluate((element, previous) => element.currentTime > previous.time + 0.4
        && element.getVideoPlaybackQuality().totalVideoFrames > previous.frames, first), { timeout: 10000 }).toBe(true);
    return video.evaluate((element) => ({ frames: element.getVideoPlaybackQuality().totalVideoFrames,
        currentTime: element.currentTime, width: element.videoWidth, height: element.videoHeight }));
}

export async function runNativeLive({ manifestPath, manifest, ready, gatewayOrigin, directory, abortSignal, verifyPlayback, armRecovery }) {
    if (typeof verifyPlayback !== 'function') throw new Error('Native live playback requires real backend verification.');
    const recoveryMode = ready.nativeRecovery === true;
    if (recoveryMode && (typeof armRecovery !== 'function' || !ready.nativeCandidate)) throw new Error('Native worker recovery requires its owned candidate and arm callback.');
    const approved = await readQualification(manifestPath || manifest?.manifestPath);
    const scenario = nativeLiveScenario(approved, ready, gatewayOrigin);
    const report = { resourceId: ready.resourceId, passed: false, checks: {}, limitations: ['The fixture seeds an existing real server session; password login is not qualified.',
        ...(recoveryMode ? [] : ['The short HLS worker completes before presentation; native automatic process failure recovery requires a separate longer fixture.']),
        'Physical device scaling, native menu/tray and broad media format fidelity remain separate.'] };
    let runtime;
    let observation;
    let isolated = false;
    let failure;
    let stage = 'launch';
    let pageErrors = 0;
    let canceled = false;
    const abort = () => { canceled = true; };
    try {
        if (abortSignal?.aborted) throw new Error('Native live qualification was canceled.');
        runtime = await launchQualification(approved);
        abortSignal?.addEventListener('abort', abort, { once: true });
        const { page } = runtime;
        page.setDefaultTimeout(15000);
        page.on('pageerror', (error) => {
            pageErrors += 1;
            report.pageErrorMessages ||= [];
            if (report.pageErrorMessages.length < 5) report.pageErrorMessages.push(nativeLiveFailure(error, scenario.token, stage));
        });
        report.browser = runtime.browser.version();
        stage = 'isolation';
        await awaitNativeFirstRun(page);
        await assertNativeIsolation(page, approved);
        isolated = true;
        if (canceled || abortSignal?.aborted) throw new Error('Native live qualification was canceled.');
        observation = observeNativeLive(page, scenario);
        stage = 'connect';
        report.checks.connection = await connectNativeLive(page, scenario);
        stage = 'decode';
        const region = page.getByRole('region', { name: 'Media player', exact: true });
        await expect(region).toBeVisible();
        const video = region.locator('video');
        await expect(video).toBeVisible();
        const play = region.getByRole('button', { name: 'Play', exact: true });
        if (await play.count() === 1) await play.click();
        report.checks.initialDecoded = await decoded(video);
        await observation.drain();
        const initial = assertNativeLiveMedia(observation.records).starts[0].reply;
        let generation = initial.transcode_session_id;
        stage = 'audio';
        await region.hover();
        await region.getByRole('button', { name: 'Pause', exact: true }).click();
        await region.getByRole('button', { name: 'Play', exact: true }).click();
        report.checks.audio = await observeNativeAudio(page, video, scenario.candidate.audioHz);
        stage = 'seek';
        await region.getByRole('button', { name: 'Pause', exact: true }).click();
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        const previousSource = await video.evaluate((element) => element.currentSrc);
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        await seek.focus();
        await page.keyboard.press('Home');
        for (let index = 0; index < 40; index += 1) await page.keyboard.press('ArrowRight');
        await page.keyboard.press('Tab');
        await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/seek' && record.status === 200).length).toBeGreaterThan(0);
        const sought = observation.records.findLast((record) => record.path === '/api/v1/playback/seek' && record.status === 200);
        if (sought.reply.position_ms !== 4000 || sought.reply.session_id && sought.reply.session_id !== initial.session_id
            || !sought.reply.transcode_session_id || sought.reply.transcode_session_id === generation) throw new Error('Native seek did not preserve the owned source position and session.');
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        let previousTranscodeId = generation;
        generation = sought.reply.transcode_session_id;
        report.checks.seek = { positionMs: sought.reply.position_ms, preservedPause: true, transcodeId: generation };
        await expect.poll(() => observation.records.some((record) => record.path.includes(`/transcode/${generation}/`)
            && record.path.endsWith('.m3u8') && record.status === 200)).toBe(true);
        await expect.poll(() => video.evaluate((element, previous) => element.currentSrc.startsWith('blob:')
            && element.currentSrc !== previous, previousSource)).toBe(true);
        const attachedAt = await page.evaluate(() => performance.now());
        report.checks.seek.freshBlobSource = true;
        if (recoveryMode) {
            stage = 'recovery';
            report.checks.recovery = await recoverPausedNative({ page, region, video, observation,
                expected: { resourceId: ready.resourceId, sessionId: initial.session_id, transcodeId: generation, positionMs: 4000, paused: true },
                armRecovery, verifyPlayback, decode: decoded, expectedHz: scenario.candidate.audioHz, abortSignal });
            generation = report.checks.recovery.transcodeId;
            stage = 'post-recovery-seek';
            const recoveredSource = await video.evaluate((element) => element.currentSrc);
            const priorSeeks = observation.records.filter((record) => record.path === '/api/v1/playback/seek' && record.status === 200).length;
            await seek.focus();
            await page.keyboard.press('Home');
            for (let index = 0; index < 60; index += 1) await page.keyboard.press('ArrowRight');
            await page.keyboard.press('Tab');
            await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/seek' && record.status === 200).length).toBe(priorSeeks + 1);
            const finalSeek = observation.records.findLast((record) => record.path === '/api/v1/playback/seek' && record.status === 200);
            if (finalSeek.reply.position_ms !== 6000 || finalSeek.reply.session_id !== initial.session_id
                || !finalSeek.reply.transcode_session_id || finalSeek.reply.transcode_session_id === generation) throw new Error('The post-recovery native seek lost its source/session ownership.');
            previousTranscodeId = generation;
            generation = finalSeek.reply.transcode_session_id;
            await expect.poll(() => observation.records.some((record) => record.path.includes(`/transcode/${generation}/`) && record.path.endsWith('.m3u8') && record.status === 200)).toBe(true);
            await expect.poll(() => video.evaluate((element, previous) => element.currentSrc.startsWith('blob:') && element.currentSrc !== previous && element.paused, recoveredSource)).toBe(true);
            const finalAttachedAt = await page.evaluate(() => performance.now());
            await region.getByRole('button', { name: 'Play', exact: true }).click();
            report.checks.seek = { positionMs: 6000, preservedPause: true, transcodeId: generation, freshBlobSource: true,
                decoded: await decoded(video), audio: await observeNativeAudio(page, video, scenario.candidate.audioHz, { attachedAt: finalAttachedAt, minimumElapsedMs: 500 }) };
            await region.getByRole('button', { name: 'Pause', exact: true }).click();
        } else {
            await region.getByRole('button', { name: 'Play', exact: true }).click();
            report.checks.seek.decoded = await decoded(video);
            report.checks.seek.audio = await observeNativeAudio(page, video, scenario.candidate.audioHz, { attachedAt, minimumElapsedMs: 500 });
            await region.getByRole('button', { name: 'Pause', exact: true }).click();
        }
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        stage = 'verification';
        if (canceled || abortSignal?.aborted) throw new Error('Native live qualification was canceled.');
        report.checks.backend = await verifyPlayback({ resourceId: ready.resourceId, phase: 'native', sessionId: initial.session_id,
            previousTranscodeId, transcodeId: generation, positionMs: report.checks.seek.positionMs,
            confirmedPositionMs: report.checks.seek.positionMs + Math.floor(await video.evaluate((element) => element.currentTime) * 1000), attempts: recoveryMode ? 1 : 0, paused: true,
            ...(recoveryMode ? { recovery: report.checks.recovery } : {}) });
        if (report.checks.backend?.passed !== true) throw new Error('Native backend verification did not pass.');
        await page.screenshot({ path: join(directory || approved.directory, 'native-live.png') });
        stage = 'close';
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe(`/media/${scenario.candidate.item}`);
        await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/stop'
            && record.status === 200 && record.request?.session_id === initial.session_id).length).toBe(1);
        await observation.drain();
        assertNativeLiveMedia(observation.records);
        if (pageErrors || abortSignal?.aborted) throw new Error('Native playback page failed or was canceled.');
        report.checks.closedCanonicalTitleAndStoppedOnce = true;
        report.checks.authenticatedHls = true;
        report.passed = true;
    } catch (error) {
        failure = new Error(`Native live playback qualification failed during ${stage}.`);
        report.error = failure.message;
        report.cause = nativeLiveFailure(error, scenario.token, stage);
    } finally {
        abortSignal?.removeEventListener('abort', abort);
        if (runtime?.page && !runtime.page.isClosed()) {
            try { await releaseNativeAudio(runtime.page); } catch { failure ||= new Error('Native audio observer cleanup failed.'); }
            try {
                const close = runtime.page.getByRole('button', { name: 'Close player', exact: true });
                if (await close.count() === 1) await close.click({ timeout: 5000 });
            } catch { report.playbackCleanupFailed = true; failure ||= new Error('Native live playback cleanup failed.'); }
            if (isolated) {
                try { report.credentialCleanup = await clearNativeLiveCredential(runtime.page, scenario); }
                catch { report.credentialCleanupFailed = true; failure ||= new Error('Native live credential cleanup failed.'); }
            }
        }
        if (observation) {
            try { await observation.drain(); } catch { failure ||= new Error('Native live observation cleanup failed.'); }
            observation.detach();
            report.requests = observation.records;
        }
        if (runtime) {
            try { report.processCleanup = await stopQualification(runtime); }
            catch { report.processCleanupFailed = true; failure ||= new Error('Native live process cleanup failed.'); }
        }
        report.pageErrors = pageErrors;
        report.passed &&= !failure;
        await writeFile(join(directory || approved.directory, 'native-live.json'), `${JSON.stringify(report, null, 2)}\n`);
    }
    if (failure) throw failure;
    return report;
}
