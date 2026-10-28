// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { installFormatPresentation } from '../fixtures/format-presentation.mjs';
import { assertFormatMedia, assertFormatPresentation, assertFormatSeek, formatSeeks, initialFormatGeneration } from './qualification-format-policy.mjs';
import { observeNativeAudio } from './qualification-live-media.mjs';
import { formatFailureSnapshot } from './qualification-format-failure.mjs';
import { pauseThroughUi, pausedThroughUi } from './qualification-pause.mjs';
export { pauseThroughUi, pausedThroughUi } from './qualification-pause.mjs';

const active = (signal) => { if (signal?.aborted) throw new Error('Format presentation qualification was canceled.'); };

async function bindPresentation(page, video, generation, previousSource = null) {
    await expect.poll(() => video.evaluate((element, previous) => element.currentSrc.startsWith('blob:')
        && (!previous || element.currentSrc !== previous), previousSource)).toBe(true);
    return page.evaluate((id) => window.formatPresentation.bind(id), generation);
}

async function presented(page, generation, signal) {
    await expect.poll(async () => {
        active(signal);
        try { assertFormatPresentation(await page.evaluate(() => window.formatPresentation.snapshot()), generation); return true; }
        catch { return false; }
    }, { timeout: 15000 }).toBe(true);
    return assertFormatPresentation(await page.evaluate(() => window.formatPresentation.snapshot()), generation);
}

async function awaitFormatMedia(observation, generation, client) {
    await expect.poll(() => {
        try { assertFormatMedia(observation.records, generation, client); return true; }
        catch { return false; }
    }).toBe(true);
}

async function seekThroughUi({ page, seek, observation, sessionId, previousTranscodeId, positionMs, client, expectedBaseline }) {
    const baseline = formatSeeks(observation.records).length;
    if (baseline !== expectedBaseline) throw new Error('Unexpected format generation rotations preceded the actual UI seek.');
    const usedGenerations = observation.records.filter((record) => record.status === 200 && ['/api/v1/playback/start', '/api/v1/playback/seek'].includes(record.path))
        .map((record) => record.reply?.transcode_session_id);
    await seek.focus();
    await page.keyboard.press('Home');
    for (let index = 0; index < positionMs / 100; index += 1) await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Tab');
    await expect.poll(() => formatSeeks(observation.records).length).toBe(baseline + 1);
    const records = formatSeeks(observation.records);
    if (records.length !== baseline + 1) throw new Error('One actual format UI action produced unexpected generation rotations.');
    const record = records[baseline];
    const generation = assertFormatSeek(record, { sessionId, previousTranscodeId, positionMs, client });
    if (usedGenerations.includes(generation)) throw new Error('The format UI seek replayed a retired generation.');
    return { generation, record, baseline };
}

async function playbackSteps({ page, region, observation, start, candidate, client, abortSignal, stage }) {
    stage('initial-response');
    active(abortSignal);
    const video = region.locator('video');
    await expect(video).toBeVisible();
    const seek = region.getByRole('slider', { name: 'Seek', exact: true });
    await expect(seek).toBeEnabled();
    await expect.poll(() => {
        try { initialFormatGeneration(observation.records, start, candidate, client); return true; }
        catch { return false; }
    }).toBe(true);
    await observation.drain();
    const initialResume = initialFormatGeneration(observation.records, start, candidate, client);
    let initialId = initialResume.generation;
    let reset = null;
    stage('initial-generation-media');
    await awaitFormatMedia(observation, initialId, client);
    stage('initial-pause-before-reset');
    const initialPause = await pauseThroughUi(region, video, initialResume.resumePositionMs > 0);
    if (initialResume.resumePositionMs > 0) {
        stage('reset-to-zero');
        await expect.poll(() => video.evaluate((element) => element.currentSrc.startsWith('blob:') && element.readyState >= 2)).toBe(true);
        const resumedSource = await video.evaluate((element) => element.currentSrc);
        reset = await seekThroughUi({ page, seek, observation, sessionId: start.reply.session_id, previousTranscodeId: initialId,
            positionMs: 0, client, expectedBaseline: initialResume.seekCount });
        initialId = reset.generation;
        stage('reset-generation-media');
        await awaitFormatMedia(observation, initialId, client);
        await expect.poll(() => video.evaluate((element, previous) => element.currentSrc.startsWith('blob:') && element.currentSrc !== previous && element.paused, resumedSource)).toBe(true);
        await pausedThroughUi(region, video);
    }
    stage('initial-presented-frames');
    await page.evaluate(installFormatPresentation);
    const initialAttachment = await bindPresentation(page, video, initialId);
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const initialFrames = await presented(page, initialId, abortSignal);
    await pauseThroughUi(region, video);
    stage('initial-selected-audio');
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const initialAudio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: initialAttachment.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    const previousSource = await video.evaluate((element) => element.currentSrc);
    stage('seek-to-five-seconds');
    const finalSeek = await seekThroughUi({ page, seek, observation, sessionId: start.reply.session_id, previousTranscodeId: initialId,
        positionMs: 5000, client, expectedBaseline: initialResume.seekCount + (reset ? 1 : 0) });
    const id = finalSeek.generation;
    stage('final-generation-media');
    await awaitFormatMedia(observation, id, client);
    const attachment = await bindPresentation(page, video, id, previousSource);
    await pausedThroughUi(region, video);
    active(abortSignal);
    stage('final-presented-frames');
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    const frames = await presented(page, id, abortSignal);
    stage('final-selected-audio');
    const audio = await observeNativeAudio(page, video, candidate.audioHz, { attachedAt: attachment.attachedAt, minimumElapsedMs: 500 });
    await pauseThroughUi(region, video);
    stage('final-paused-proof');
    await observation.drain();
    if (formatSeeks(observation.records).length !== finalSeek.baseline + 1) throw new Error('Unexpected format generation rotations followed the actual UI seek.');
    assertFormatMedia(observation.records, initialId, client);
    assertFormatMedia(observation.records, id, client);
    return { sessionId: start.reply.session_id, transcodeId: id, previousTranscodeId: initialId, positionMs: 5000, paused: true, attempts: 0,
        initialResume, initialPause, reset, finalSeek, initialFrames, initialAudio, frames, audio, preservedPause: true, freshBlobSource: true };
}

export async function qualifyFormatPlayback(options) {
    let stage = 'initial';
    try { return await playbackSteps({ ...options, stage: (value) => { stage = value; } }); }
    catch (error) {
        error.formatFailure = await formatFailureSnapshot(options.page, options.observation, stage);
        throw error;
    }
}
