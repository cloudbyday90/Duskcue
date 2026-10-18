// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { assertNativeLiveMedia } from './qualification-live-policy.mjs';
import { assertNativeRecoveryArm, assertNaturalNativeRecovery } from './qualification-live-recovery-policy.mjs';
import { observeNativeAudio } from './qualification-live-media.mjs';

const notice = 'A more compatible playback stream is ready.';
const requireActive = (signal) => { if (signal?.aborted) throw new Error('Native worker recovery qualification was canceled.'); };

export async function recoverPausedNative({ page, region, video, observation, expected, armRecovery, verifyPlayback, decode, expectedHz, abortSignal }) {
    requireActive(abortSignal);
    const previousSource = await video.evaluate((element) => element.currentSrc);
    if (!previousSource.startsWith('blob:') || !await video.evaluate((element) => element.paused)) throw new Error('The native recovery source must already be attached and paused.');
    await video.evaluate((element) => {
        const observed = { video: element, plays: 0, listener: null };
        observed.listener = () => { if (!element.paused) observed.plays += 1; };
        element.addEventListener('play', observed.listener);
        window.nativeLiveRecoveryPause = observed;
    });
    const firstRecord = observation.records.length;
    let armed;
    let heartbeat;
    try {
        armed = assertNativeRecoveryArm(await armRecovery(expected), expected);
        requireActive(abortSignal);
        await expect.poll(() => {
            requireActive(abortSignal);
            heartbeat = observation.records.slice(firstRecord).find((record) => record.path === '/api/v1/playback/heartbeat'
                && record.status === 200 && record.request?.session_id === expected.sessionId
                && record.request.transcode_session_id === expected.transcodeId && record.reply?.transcode_session_id
                && record.reply.transcode_session_id !== expected.transcodeId);
            return Boolean(heartbeat);
        }, { timeout: 75000 }).toBe(true);
        assertNaturalNativeRecovery(heartbeat, expected);
        await observation.drain();
        assertNativeLiveMedia(observation.records);
        const id = heartbeat.reply.transcode_session_id;
        await expect.poll(() => {
            requireActive(abortSignal);
            return observation.records.some((record) => record.path.includes(`/transcode/${id}/`) && record.path.endsWith('.m3u8') && record.status === 200);
        }).toBe(true);
        await expect.poll(() => {
            requireActive(abortSignal);
            return video.evaluate((element, previous) => element.currentSrc.startsWith('blob:') && element.currentSrc !== previous && element.paused, previousSource);
        }).toBe(true);
        await expect(region.locator('.recovery-status')).toBeVisible();
        await expect(region.locator('.recovery-status')).toHaveText(notice);
        await expect(region.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
        const backend = await verifyPlayback({ resourceId: expected.resourceId, phase: 'native-recovery', sessionId: expected.sessionId,
            previousTranscodeId: expected.transcodeId, transcodeId: id, positionMs: 4000, confirmedPositionMs: 4000, attempts: 1, paused: true });
        if (backend?.passed !== true) throw new Error('The paused native recovery checkpoint did not pass backend verification.');
        requireActive(abortSignal);
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        const unexpectedPlayEvents = await page.evaluate(() => window.nativeLiveRecoveryPause.plays);
        if (unexpectedPlayEvents !== 0) throw new Error('The paused native player resumed during automatic recovery.');
        const attachedAt = await page.evaluate(() => performance.now());
        await video.evaluate((element) => {
            element.removeEventListener('play', window.nativeLiveRecoveryPause.listener);
            delete window.nativeLiveRecoveryPause;
        });
        requireActive(abortSignal);
        await region.getByRole('button', { name: 'Play', exact: true }).click();
        const decoded = await decode(video);
        const audio = await observeNativeAudio(page, video, expectedHz, { attachedAt, minimumElapsedMs: 500 });
        await region.getByRole('button', { name: 'Pause', exact: true }).click();
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        return { armed, heartbeat, previousTranscodeId: expected.transcodeId, transcodeId: id, sessionId: expected.sessionId,
            positionMs: 4000, attempts: 1, preservedPause: true, unexpectedPlayEvents, notice, freshBlobSource: true, backend, decoded, audio };
    } finally {
        await page.evaluate(() => {
            const observed = window.nativeLiveRecoveryPause;
            if (observed) observed.video.removeEventListener('play', observed.listener);
            delete window.nativeLiveRecoveryPause;
        });
    }
}
