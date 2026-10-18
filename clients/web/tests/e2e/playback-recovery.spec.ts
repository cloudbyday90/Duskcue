import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;
const failureText = 'This device could not play the stream. Retry or return to the title.';

test('preference failure blocks start until a confirmed saved profile default can be read', async ({ page, api, playback }) => {
    const path = 'GET /profiles/current/viewing-preferences';
    api.failures.set(path, { status: 503, detail: 'Fixture viewing preferences unavailable' });
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    await expect(region.getByText('Viewing preferences could not be loaded. Retry before starting playback.', { exact: true })).toBeVisible();
    expect(playback.starts).toHaveLength(0);
    api.failures.delete(path);
    await region.getByRole('button', { name: 'Retry', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expectDecodedPlayback(page);
    await expect(region).toBeFocused();
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
});

test('a native direct video source failure offers Retry without losing the Title destination', async ({ page, api, playback }) => {
    const path = `/stream/${api.files[mediaIds.resumeMovie][0].id}`;
    api.failures.set(`GET ${path}`, { status: 503, detail: 'Fixture media unavailable' });
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    await expect.poll(() => playback.failureResponses).toContainEqual({ method: 'GET', path, status: 503 });
    await expect(region.getByText(failureText, { exact: true })).toBeVisible();
    api.failures.delete(`GET ${path}`);
    await region.getByRole('button', { name: 'Retry', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expectDecodedPlayback(page);
    await expect(region).toBeFocused();
    expect(playback.starts).toHaveLength(2);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
    expect(playback.stops).toHaveLength(2);
});

test.describe('real HLS source failure', () => {
    test.use({ playbackOptions: { mode: 'transcode' } });

    test('a rejected manifest becomes recoverable instead of endlessly restarting HLS loading', async ({ page, api, playback }) => {
        const path = '/transcode/00000000-0000-7000-8000-000000000701/manifest.m3u8';
        api.failures.set(`GET ${path}`, { status: 401, detail: 'Fixture media access denied' });
        const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
        await expect.poll(() => playback.failureResponses.filter((failure) => failure.method === 'GET' && failure.path === path && failure.status === 401).length).toBeGreaterThan(0);
        await expect(region.getByText(failureText, { exact: true })).toBeVisible({ timeout: 15_000 });
        await expect(region.getByRole('button', { name: 'Retry', exact: true })).toBeVisible();
        const failedSession = playback.starts[0].session_id;
        api.failures.delete(`GET /transcode/${failedSession}/manifest.m3u8`);
        await region.getByRole('button', { name: 'Retry', exact: true }).focus();
        await page.keyboard.press('Enter');
        await expectDecodedPlayback(page);
        await expect(region).toBeFocused();
        expect(playback.starts).toHaveLength(2);
        expect(playback.stops.filter((stop) => stop.session_id === failedSession)).toHaveLength(1);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, titleUrl);
    });
});

test.describe('replacement HLS play intent', () => {
    test.use({ playbackOptions: { mode: 'transcode', seekReplacement: true } });

    test('Pause while a replacement manifest is pending stays paused after decoding', async ({ page, playback }) => {
        const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        const originalSource = await video.getAttribute('src');
        const manifest = /\/api\/v1\/transcode\/[^/]+\/manifest\.m3u8$/;
        let release!: () => void;
        const held = new Promise<void>((resolve) => { release = resolve; });
        let requested = false;
        await page.route(manifest, async (route) => {
            requested = true;
            await held;
            await route.fallback();
        });
        try {
            await region.focus();
            await page.keyboard.press('ArrowRight');
            await expect.poll(() => requested).toBe(true);
            await expect.poll(() => video.getAttribute('src')).not.toBe(originalSource);
            await region.getByRole('button', { name: 'Pause', exact: true }).focus();
            await page.keyboard.press('Enter');
            await expect(region.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
            release();
            await expect.poll(() => video.evaluate((element: HTMLVideoElement) => ({ width: element.videoWidth, ready: element.readyState >= 2, paused: element.paused })), { timeout: 10_000 }).toEqual({ width: 320, ready: true, paused: true });
            expect(playback.starts).toHaveLength(1);
            expect(playback.seeks).toHaveLength(1);
            await region.getByRole('button', { name: 'Close player', exact: true }).click();
            await expectTitleContext(page, titleUrl);
        } finally {
            release();
            await page.unroute(manifest);
        }
    });
});
