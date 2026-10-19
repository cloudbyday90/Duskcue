// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { pauseThroughUi, pausedThroughUi } from './qualification-format-case.mjs';
import { waitForFormatPacing } from './qualification-format-pacing.mjs';
import { formatFailureSnapshot } from './qualification-format-failure.mjs';
import { observeNativeAudio, releaseNativeAudio } from './qualification-live-media.mjs';
import { installSourcePresentation } from './qualification-source-presentation.mjs';
import { assertSourceMedia, assertSourcePresentation, assertSourceSeek, sourceSeeks } from './qualification-source-policy.mjs';
import { sourceQuality, setNativeQualificationQuality } from './qualification-source-quality.mjs';
import { assertSourceDecoderRecovery, sourceRecoveryCandidate, sourceRecoveryContract } from './qualification-source-recovery-policy.mjs';
import { installSourceRecoveryErrors, observeSourceRecoveryRequests } from './qualification-source-recovery-observation.mjs';
import { waitForSourceRecoveryStartup } from './qualification-source-recovery-startup.mjs';
import { preserveSourceRecoveryFailure, sourceRecoveryReleaseEvidence } from './qualification-source-recovery-cleanup.mjs';

const active = (signal) => { if (signal?.aborted) throw new Error('Actual source decoder recovery was canceled.'); };

async function readyMedia(video, observation, candidate, generation, client, previousSource = null, startup = false, signal = undefined) {
    if (startup) {
        await waitForSourceRecoveryStartup({ generation, signal, previousSource, readRecords: () => observation.records,
            readMedia: async () => {
                const media = await video.evaluate((element) => ({ ready: element.readyState, source: element.currentSrc }));
                try { assertSourceMedia(observation.records, candidate, generation, client, false); }
                catch { return { ...media, ready: 0 }; }
                return media;
            } });
        return;
    }
    await expect.poll(() => {
        try { assertSourceMedia(observation.records, candidate, generation, client, false); return true; } catch { return false; }
    }, { timeout: 20000 }).toBe(true);
    await expect.poll(() => video.evaluate((element, previous) => element.readyState >= 2 && element.currentSrc.startsWith('blob:')
        && (!previous || element.currentSrc !== previous), previousSource)).toBe(true);
}

async function frames(page, generation, descriptor, signal, targetPositionMs = null, streamOffsetMs = 0) {
    const expected = { owner: generation, native: false, width: descriptor.video.width, height: descriptor.video.height, targetPositionMs, streamOffsetMs };
    await expect.poll(async () => {
        active(signal);
        try { assertSourcePresentation(await page.evaluate(() => window.sourcePresentation.snapshot()), expected); return true; } catch { return false; }
    }, { timeout: 15000 }).toBe(true);
    return assertSourcePresentation(await page.evaluate(() => window.sourcePresentation.snapshot()), expected);
}

async function recoverySteps({ page, ready, origin, client, candidate, observation, verifySource, abortSignal, stage }) {
    active(abortSignal);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    await expect(region).toBeVisible();
    const video = region.locator('video');
    await expect(video).toBeVisible();
    await page.evaluate(installSourceRecoveryErrors);
    await page.evaluate(installSourcePresentation);
    stage('prepare-start');
    await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/start').length, { timeout: 20000 }).toBe(1);
    await expect.poll(() => observation.records.filter((record) => record.path === '/api/v1/playback/prepare').length).toBe(1);
    await expect.poll(() => observation.records.filter((record) => record.method === 'GET' && record.path === `/api/v1/items/${candidate.item}/watch-data`).length).toBe(1);
    const contract = sourceRecoveryContract(observation.records, candidate, client);
    const generation = contract.start.reply.transcode_session_id;
    const sessionId = contract.start.reply.session_id;
    const descriptor = contract.start.reply.copy_fallback;
    stage('healthy-first-frame');
    await readyMedia(video, observation, candidate, generation, client, null, true, abortSignal);
    const attached = await page.evaluate((value) => window.sourcePresentation.bind(value), { owner: generation, native: false, fileId: candidate.file });
    await region.hover();
    const play = region.getByRole('button', { name: 'Play', exact: true });
    if (await play.isVisible()) await play.click();
    await expect.poll(() => page.evaluate(() => {
        const media = window.sourcePresentation.snapshot();
        return media.ready >= 2 && media.frames > 0 && !media.paused && !media.ended;
    })).toBe(true);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    await pausedThroughUi(region, video);
    const first = await page.evaluate(() => window.sourcePresentation.snapshot());
    const offset = contract.start.reply.stream_offset_ms ?? 0;
    const positionMs = Math.floor(first.currentTime * 1000 + offset);
    if (!first.paused || first.ended || first.frames < 1 || !Number.isSafeInteger(positionMs) || positionMs < 0 || positionMs >= 1500
        || sourceSeeks(observation.records).length || (await page.evaluate(() => window.sourceRecoveryErrors.snapshot())).length) throw new Error('The source failed or advanced before its real healthy-copy checkpoint.');
    const context = { resourceId: ready.resourceId, caseId: candidate.variant, itemId: candidate.item, fileId: candidate.file,
        preparation: contract.preparation, observations: contract.observations, route: contract.route, decision: contract.start.reply.stream_decision,
        capabilityProtocol: 1, qualityMode: 'auto', transport: client === 'native' ? 'bearer' : 'cookie' };
    stage('healthy-copy-backend-arm');
    const armed = await verifySource({ ...context, phase: `source-recovery-armed-${client}`, sessionId,
        transcodeId: generation, positionMs, paused: true, attempts: 0 });
    if (armed?.passed !== true || armed.armed !== true || armed.armed_generation !== generation || armed.healthy_copy !== true || armed.attempts !== 0) throw new Error('The actual healthy copied worker did not pass its backend checkpoint.');
    await page.evaluate((id) => window.sourceRecoveryErrors.arm(id), generation);
    stage('natural-decoder-failure');
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(async () => {
        active(abortSignal);
        try {
            assertSourceDecoderRecovery(observation.records, sessionId, generation, await page.evaluate(() => window.sourceRecoveryErrors.snapshot()), client);
            return true;
        } catch { return false; }
    }, { timeout: 75000 }).toBe(true);
    const recovered = assertSourceDecoderRecovery(observation.records, sessionId, generation,
        await page.evaluate(() => window.sourceRecoveryErrors.snapshot()), client);
    if (sourceSeeks(observation.records).length) throw new Error('A UI/API seek substituted for natural copied-source failure recovery.');
    stage('replacement-media');
    await readyMedia(video, observation, candidate, recovered.generation, client, recovered.decoderError.source);
    await expect(region.locator('.recovery-status')).toBeVisible();
    const visibleNotice = await region.locator('.recovery-status').innerText();
    if (!visibleNotice.trim()) throw new Error('The actual compatible recovery notice is missing.');
    const replacement = await page.evaluate((value) => window.sourcePresentation.bind(value), { owner: recovered.generation, native: false, fileId: candidate.file });
    const replacementFrames = await frames(page, recovered.generation, descriptor, abortSignal);
    const replacementAudio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: replacement.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    stage('post-recovery-ui-seek');
    const previousSource = await video.evaluate((element) => element.currentSrc);
    const seek = region.getByRole('slider', { name: 'Seek', exact: true });
    await seek.focus();
    await page.keyboard.press('Home');
    for (let index = 0; index < 50; index += 1) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Tab');
    await expect.poll(() => sourceSeeks(observation.records).length).toBe(1);
    const sought = sourceSeeks(observation.records)[0];
    const finalGeneration = assertSourceSeek(sought, { sessionId, previousOwner: recovered.generation, positionMs: 5000, client, native: false });
    if (sought.reply.copy_fallback != null) throw new Error('The post-recovery seek reverted to the failed copy route.');
    await readyMedia(video, observation, candidate, finalGeneration, client, previousSource);
    await pausedThroughUi(region, video);
    const finalAttachment = await page.evaluate((value) => window.sourcePresentation.bind(value), { owner: finalGeneration, native: false, fileId: candidate.file });
    stage('post-recovery-presentation');
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const finalFrames = await frames(page, finalGeneration, descriptor, abortSignal, 5000, sought.reply.stream_offset_ms);
    const finalAudio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: finalAttachment.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    const confirmedPositionMs = Math.floor(await video.evaluate((element) => element.currentTime) * 1000 + sought.reply.stream_offset_ms);
    stage('post-recovery-backend-verification');
    const backend = await verifySource({ ...context, phase: `source-recovery-${client}`, sessionId, failedTranscodeId: generation,
        previousTranscodeId: recovered.generation, transcodeId: finalGeneration, positionMs: 5000, confirmedPositionMs, paused: true, attempts: 1,
        replacementOffsetMs: recovered.replacementOffsetMs, recoveryPositionMs: recovered.recoveryPositionMs,
        decoderError: recovered.decoderError, decoderRequest: recovered.heartbeat.request, recovery: recovered.recovery });
    if (backend?.passed !== true) throw new Error('Actual source decoder recovery did not pass independent backend verification.');
    return { passed: true, sessionId, failedTranscodeId: generation, previousTranscodeId: recovered.generation, transcodeId: finalGeneration,
        positionMs: 5000, confirmedPositionMs, attempts: 1, recovery: recovered.recovery, replacementOffsetMs: recovered.replacementOffsetMs,
        recoveryPositionMs: recovered.recoveryPositionMs, decoderError: recovered.decoderError, decoderRequest: recovered.heartbeat.request,
        first, attached, armed, replacementFrames, replacementAudio, frames: finalFrames, audio: finalAudio, visibleNotice, backend,
        preparation: contract.preparation, observations: contract.observations, route: contract.route, initialWatch: contract.watch };
}

export async function runSourceRecovery({ page, ready, origin, client, directory, verifySource, abortSignal, open, close }) {
    if (ready.sourceCapabilities !== true || typeof verifySource !== 'function') throw new Error('Real source decoder recovery requires its owned protocol and verifier.');
    const candidate = sourceRecoveryCandidate(ready, client);
    const result = { resourceId: ready.resourceId, client, candidate, passed: false };
    const observation = observeSourceRecoveryRequests(page, { origin, token: ready.token });
    let stage = 'initial-refill';
    let failure;
    let closed = false;
    try {
        await waitForFormatPacing(30000, abortSignal);
        stage = 'open';
        await open(candidate);
        result.proof = await recoverySteps({ page, ready, origin, client, candidate, observation, verifySource, abortSignal, stage: (value) => { stage = value; } });
        stage = 'close';
        await close(candidate);
        closed = true;
        await observation.drain();
        if (observation.records.filter((record) => record.path === '/api/v1/playback/stop' && record.status === 200
            && record.request?.session_id === result.proof.sessionId).length !== 1) throw new Error('The recovered logical play session was not closed exactly once.');
        result.passed = true;
    } catch (error) {
        failure = error;
        result.error = String(error?.message || 'Actual source recovery failed.').replaceAll(ready.token, '[redacted]').slice(0, 1200);
        result.failure = { ...await formatFailureSnapshot(page, observation, stage),
            decoderErrors: await page.evaluate(() => window.sourceRecoveryErrors?.snapshot() || []).catch(() => []),
            presentation: await page.evaluate(() => window.sourcePresentation?.snapshot() || null).catch(() => null) };
        await page.screenshot({ path: join(directory, `source-recovery-${client}-failed.png`) }).catch(() => {});
    } finally {
        const failures = [];
        for (const [name, cleanup] of [
            ['presentation', () => page.evaluate(() => { window.sourcePresentation?.detach(); window.sourceRecoveryErrors?.detach(); })],
            ['audio', () => releaseNativeAudio(page)], ['playback', async () => { if (!closed) await close(candidate); }], ['observation', () => observation.drain()],
        ]) { try { await cleanup(); } catch { failures.push(name); } }
        observation.detach();
        result.requests = observation.records;
        result.cleanup = sourceRecoveryReleaseEvidence(observation.records, failures);
        if (result.cleanup.failures.length) {
            result.passed = false;
            if (!failure) failure = new Error('Actual source recovery cleanup failed.');
        }
        await writeFile(join(directory, `source-recovery-${client}.json`), `${JSON.stringify(result, null, 2)}\n`);
    }
    if (failure) throw failure;
    return { passed: true, client, caseId: candidate.variant, sessionId: result.proof.sessionId, attempts: 1,
        decoderCode: result.proof.decoderError.code, genuine: result.proof.decoderError.trusted, positionMs: 5000,
        healthyCopy: result.proof.armed.healthy_copy, backend: result.proof.backend.passed, cleanup: result.cleanup.passed };
}

export async function runSourceRecoveryBrowser(options) {
    const { page, ready } = options;
    const fixture = { profileId: ready.profileId, userId: ready.userId };
    const previous = await page.evaluate((fixture) => window.directFixture.setViewingPreferences(fixture,
        { audio_language: 'fr', prefer_audio_description: false, subtitle_mode: 'none', subtitle_language: null }), fixture);
    return preserveSourceRecoveryFailure(async () => {
        const previousQuality = await page.evaluate(({ fixture, quality }) => window.directFixture.setQuality(fixture, quality), { fixture, quality: sourceQuality });
        return preserveSourceRecoveryFailure(() => runSourceRecovery({ ...options, client: 'browser',
            open: (candidate) => page.evaluate(({ fixture, candidate }) => window.directFixture.mountFormat(fixture, candidate), { fixture, candidate }),
            async close() {
                const close = page.getByRole('button', { name: 'Close player', exact: true });
                if (await close.count() === 1) await close.click();
                await page.evaluate(() => window.directFixture.stop());
            } }), () => page.evaluate(({ fixture, previousQuality }) => window.directFixture.setQuality(fixture, previousQuality), { fixture, previousQuality }));
    }, () => page.evaluate(({ fixture, previous }) => window.directFixture.setViewingPreferences(fixture, previous), { fixture, previous }));
}

export async function runSourceRecoveryNative(options) {
    const { page } = options;
    const origin = new URL(page.url()).origin;
    const previous = await setNativeQualificationQuality(page, sourceQuality);
    return preserveSourceRecoveryFailure(() => runSourceRecovery({ ...options, client: 'native',
        open: (candidate) => page.goto(`${origin}/play/${candidate.item}?file=${candidate.file}`),
        async close(candidate) {
            const close = page.getByRole('button', { name: 'Close player', exact: true });
            if (await close.count() === 1) await close.click();
            else if (await page.getByRole('region', { name: 'Media player', exact: true }).count() === 0) return;
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/media/${candidate.item}`);
        } }), () => setNativeQualificationQuality(page, previous));
}
