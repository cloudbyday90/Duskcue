// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { pausedThroughUi, pauseThroughUi } from './qualification-format-case.mjs';
import { formatFailureSnapshot } from './qualification-format-failure.mjs';
import { observeNativeAudio } from './qualification-live-media.mjs';
import { installSourcePresentation } from './qualification-source-presentation.mjs';
import { assertSourceMedia, assertSourcePresentation, assertSourceSeek, initialSourceOwnership, sourceOwnership, sourceSeeks } from './qualification-source-policy.mjs';

const active = (signal) => { if (signal?.aborted) throw new Error('Source playback qualification was canceled.'); };

async function awaitMedia({ page, video, observation, candidate, owner, client, native, previousSource = null, positionMs = null }) {
    await expect.poll(() => {
        try { assertSourceMedia(observation.records, candidate, owner, client, native); return true; } catch { return false; }
    }).toBe(true);
    const expected = native ? `/api/v1/stream/${candidate.file}` : null;
    await expect.poll(() => video.evaluate((element, options) => element.readyState >= 2 && !element.seeking
        && (options.native ? element.currentSrc === new URL(options.expected, location.origin).href
            && (options.positionMs === null || Math.abs(element.currentTime * 1000 - options.positionMs) < 250)
            : element.currentSrc.startsWith('blob:') && (!options.previousSource || element.currentSrc !== options.previousSource)),
    { native, expected, previousSource, positionMs })).toBe(true);
}

async function bind(page, owner, native, candidate) {
    return page.evaluate((value) => window.sourcePresentation.bind(value), { owner, native, fileId: candidate.file });
}

async function presented(page, owner, native, candidate, signal, targetPositionMs = null, streamOffsetMs = 0) {
    const expected = { owner, native, width: candidate.capability.video.width, height: candidate.capability.video.height, targetPositionMs, streamOffsetMs };
    await expect.poll(async () => {
        active(signal);
        try { assertSourcePresentation(await page.evaluate(() => window.sourcePresentation.snapshot()), expected); return true; } catch { return false; }
    }, { timeout: 15000 }).toBe(true);
    return assertSourcePresentation(await page.evaluate(() => window.sourcePresentation.snapshot()), expected);
}

async function seekThroughUi({ page, seek, observation, contract, previousOwner, positionMs, client, expectedBaseline }) {
    const baseline = sourceSeeks(observation.records).length;
    if (baseline !== expectedBaseline) throw new Error('Unexpected source rotations preceded the deliberate UI seek.');
    const previousOwners = observation.records.filter((record) => record.status === 200 && ['/api/v1/playback/start', '/api/v1/playback/seek'].includes(record.path))
        .map((record) => sourceOwnership(record.reply, contract.native));
    await seek.focus();
    await page.keyboard.press('Home');
    for (let index = 0; index < positionMs / 100; index += 1) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Tab');
    await expect.poll(() => sourceSeeks(observation.records).length).toBe(baseline + 1);
    const seeks = sourceSeeks(observation.records);
    if (seeks.length !== baseline + 1) throw new Error('One source UI seek produced unexpected rotations.');
    const record = seeks[baseline];
    const owner = assertSourceSeek(record, { sessionId: contract.start.reply.session_id, previousOwner, positionMs, client, native: contract.native });
    if (previousOwners.includes(owner)) throw new Error('A source seek replayed a retired owner.');
    return { owner, record, baseline };
}

async function playbackSteps({ page, region, observation, contract, candidate, client, abortSignal, stage }) {
    active(abortSignal);
    const video = region.locator('video');
    const seek = region.getByRole('slider', { name: 'Seek', exact: true });
    await expect(video).toBeVisible();
    await expect(seek).toBeEnabled();
    stage('initial-resume');
    await expect.poll(() => {
        try { initialSourceOwnership(observation.records, contract, candidate, client); return true; } catch { return false; }
    }).toBe(true);
    await observation.drain();
    const resume = initialSourceOwnership(observation.records, contract, candidate, client);
    let owner = resume.owner;
    stage('initial-media');
    await awaitMedia({ page, video, observation, candidate, owner, client, native: contract.native });
    stage('initial-trusted-pause');
    const initialPause = await pauseThroughUi(region, video, resume.positionMs > 0);
    let reset = null;
    if (resume.positionMs > 0) {
        stage('reset-to-zero');
        const previousSource = await video.evaluate((element) => element.currentSrc);
        reset = await seekThroughUi({ page, seek, observation, contract, previousOwner: owner, positionMs: 0, client, expectedBaseline: resume.seekCount });
        owner = reset.owner;
        await awaitMedia({ page, video, observation, candidate, owner, client, native: contract.native, previousSource, positionMs: 0 });
        await pausedThroughUi(region, video);
    }
    stage('initial-presented-frames');
    await page.evaluate(installSourcePresentation);
    const initialAttachment = await bind(page, owner, contract.native, candidate);
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const initialFrames = await presented(page, owner, contract.native, candidate, abortSignal);
    stage('initial-selected-audio');
    const initialAudio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: initialAttachment.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    const previousSource = await video.evaluate((element) => element.currentSrc);
    stage('seek-to-five-seconds');
    const finalSeek = await seekThroughUi({ page, seek, observation, contract, previousOwner: owner, positionMs: 5000, client,
        expectedBaseline: resume.seekCount + (reset ? 1 : 0) });
    const finalOwner = finalSeek.owner;
    stage('final-media');
    await awaitMedia({ page, video, observation, candidate, owner: finalOwner, client, native: contract.native, previousSource, positionMs: 5000 });
    await pausedThroughUi(region, video);
    const attachment = await bind(page, finalOwner, contract.native, candidate);
    stage('final-presented-frames');
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const streamOffsetMs = contract.native ? 0 : finalSeek.record.reply.stream_offset_ms;
    const frames = await presented(page, finalOwner, contract.native, candidate, abortSignal, 5000, streamOffsetMs);
    stage('final-selected-audio');
    const audio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: attachment.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    await observation.drain();
    if (sourceSeeks(observation.records).length !== finalSeek.baseline + 1) throw new Error('Unexpected source rotations followed the measured UI seek.');
    assertSourceMedia(observation.records, candidate, finalOwner, client, contract.native);
    return { sessionId: contract.start.reply.session_id, transcodeId: contract.native ? null : finalOwner,
        previousTranscodeId: contract.native ? null : owner, directSourceRevision: contract.native ? finalOwner : null,
        previousDirectSourceRevision: contract.native ? owner : null, positionMs: 5000, paused: true, attempts: 0,
        streamOffsetMs,
        resume, reset, initialPause, finalSeek, initialFrames, initialAudio, frames, audio,
        preservedPause: true, nativeSourceUnchanged: contract.native && previousSource === frames.source,
        freshBlobSource: !contract.native && previousSource !== frames.source };
}

export async function qualifySourcePlayback(options) {
    let stage = 'initial';
    try { return await playbackSteps({ ...options, stage: (value) => { stage = value; } }); }
    catch (error) {
        error.sourceFailure = { ...await formatFailureSnapshot(options.page, options.observation, stage),
            presentation: await options.page.evaluate(() => window.sourcePresentation?.snapshot() || null).catch(() => null) };
        throw error;
    }
}
