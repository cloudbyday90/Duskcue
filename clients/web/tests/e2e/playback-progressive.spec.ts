/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const episodeUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent('/media?type=series')}`;
const movieUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;

test.use({ playbackOptions: { mode: 'transcode', progressiveHls: true, autoplay: true } });

test('an already produced EVENT window starts at zero rather than its live edge', async ({ page, playback }) => {
    await page.addInitScript(() => {
        const observed = new WeakSet<HTMLVideoElement>();
        const observe = (event: Event) => {
            const element = event.target;
            if (!event.isTrusted || !(element instanceof HTMLVideoElement) || observed.has(element)) return;
            observed.add(element);
            element.requestVideoFrameCallback((_now, metadata) => {
                element.dataset.firstPresentedTime = String(metadata.mediaTime);
                element.dataset.firstPresentedFrames = String(metadata.presentedFrames);
                element.dataset.firstPresentedSource = element.currentSrc;
            });
        };
        document.addEventListener('play', observe, true);
        document.addEventListener('loadedmetadata', observe, true);
    });
    const progressive = playback.progressive!;
    progressive.publish(progressive.state().count);
    const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    await expect(video).toHaveAttribute('data-first-presented-time', /^\d/);
    const frame = await video.evaluate((element: HTMLVideoElement) => ({ time: Number(element.dataset.firstPresentedTime), frames: Number(element.dataset.firstPresentedFrames), source: element.dataset.firstPresentedSource, currentSource: element.currentSrc }));
    await test.info().attach('initial-presented-frame', { body: JSON.stringify(frame), contentType: 'application/json' });
    expect(frame.time).toBeLessThan(2);
    expect(frame.source).toBe(frame.currentSource);
    await expect(region.getByRole('slider', { name: 'Seek', exact: true })).toHaveAttribute('max', '20000');
    expect(progressive.state().complete).toBe(false);
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, movieUrl);
    expect(playback.stops).toHaveLength(1);
});

test('EVENT growth preserves known runtime and final publication waits for natural episode ended', async ({ page, playback }) => {
    const progressive = playback.progressive!;
    const region = await openPlayback(page, episodeUrl, mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    const seek = region.getByRole('slider', { name: 'Seek', exact: true });
    await expect(seek).toHaveAttribute('max', '20000');
    expect(await video.evaluate((element: HTMLVideoElement) => element.duration)).toBeLessThan(8);
    await video.evaluate((element: HTMLVideoElement) => { element.dataset.progressiveOwned = 'true'; element.pause(); });
    const initialTime = await video.evaluate((element: HTMLVideoElement) => element.currentTime);
    const initialSource = await video.getAttribute('src');
    progressive.publish(6);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.duration), { timeout: 10_000 }).toBeGreaterThan(10);
    await expect(seek).toHaveAttribute('max', '20000');
    expect(await video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeCloseTo(initialTime, 2);
    await expect(video).toHaveAttribute('data-progressive-owned', 'true');
    expect(await video.getAttribute('src')).toBe(initialSource);
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toHaveCount(0);
    progressive.complete();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.duration), { timeout: 10_000 }).toBeGreaterThan(19);
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toHaveCount(0);
    expect(playback.starts).toHaveLength(1);
    expect(playback.stops).toHaveLength(0);
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    await video.evaluate((element: HTMLVideoElement) => { delete element.dataset.nativeEnded; element.currentTime = element.duration - 0.25; });
    await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.ended)).toBe(true);
    const playNow = region.getByRole('button', { name: 'Play now', exact: true });
    await expect(playNow).toBeVisible();
    await playNow.focus();
    await region.getByRole('button', { name: 'Cancel', exact: true }).click();
    await expect(region.getByRole('button', { name: 'Play next', exact: true })).toBeVisible();
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, episodeUrl);
    expect(playback.stops).toHaveLength(1);
    expect(playback.stops[0].session_id).toBe(playback.starts[0].session_id);
});

test('quality replacement owns its new EVENT source and keeps known runtime through completion', async ({ page, playback }) => {
    const progressive = playback.progressive!;
    const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    await video.evaluate((element: HTMLVideoElement) => { element.dataset.progressiveOwned = 'true'; });
    const previousSource = await video.getAttribute('src');
    await region.getByRole('button', { name: 'Settings', exact: true }).click();
    await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Highest available quality', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    await expect(video).toHaveAttribute('data-progressive-owned', 'true');
    await expect.poll(() => video.getAttribute('src')).not.toBe(previousSource);
    await expect(region.getByRole('slider', { name: 'Seek', exact: true })).toHaveAttribute('max', '20000');
    expect(playback.stops.map((stop) => stop.session_id)).toEqual([playback.starts[0].session_id]);
    progressive.complete();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.duration), { timeout: 10_000 }).toBeGreaterThan(19);
    await video.evaluate((element: HTMLVideoElement) => { delete element.dataset.nativeEnded; element.currentTime = element.duration - 0.25; });
    await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.ended)).toBe(true);
    await expect(region.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toHaveCount(0);
    expect(playback.starts.map((start) => start.media_item_id)).toEqual([mediaIds.resumeMovie, mediaIds.resumeMovie]);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, movieUrl);
    expect(playback.stops.map((stop) => stop.session_id)).toEqual(playback.starts.map((start) => start.session_id));
});
