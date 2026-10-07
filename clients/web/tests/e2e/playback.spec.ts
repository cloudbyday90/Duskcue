import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const movieUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie&sort=title&order=asc&watch=unwatched')}`;
const episodeOrigin = '/search?q=Harbor&type=series&sort=title&order=asc&watch=unwatched&pages=2';
const episodeUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent(episodeOrigin)}`;

test.describe('real direct media', () => {
    test.use({ playbackOptions: { resumePositionMs: 1000 } });

    test('decodes a real MP4, resumes fresh watch data, and closes once to the full Title', async ({ page, api, playback }) => {
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeGreaterThan(1);
        await region.getByRole('button', { name: 'Pause', exact: true }).click();
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
        await page.screenshot({ path: test.info().outputPath('player-minimal-paused.png') });
        const position = await video.evaluate((element: HTMLVideoElement) => element.currentTime * 1000);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, movieUrl);
        await expect.poll(() => playback.stops.length).toBe(1);
        expect(playback.starts).toHaveLength(1);
        expect(playback.starts[0]).toMatchObject({ media_item_id: mediaIds.resumeMovie, media_file_id: api.files[mediaIds.resumeMovie][0].id });
        expect(playback.stops[0].position_ms).toBeGreaterThanOrEqual(Math.floor(position) - 300);
        expect(api.watchByProfile[profileIds.alex][mediaIds.resumeMovie].resume_position_ms).toBe(playback.stops[0].position_ms);
        expect(api.watchByProfile[profileIds.morgan][mediaIds.resumeMovie].resume_position_ms).toBe(0);
        await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
        expect(api.requests.some((request) => request.path === `/stream/${api.files[mediaIds.resumeMovie][0].id}`)).toBe(true);
    });

    test('native keyboard seek and volume retain focus without double-running shortcuts', async ({ page, api, playback }) => {
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Pause', exact: true }).focus();
        await page.keyboard.press('Enter');
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        await seek.focus();
        const before = Number(await seek.inputValue());
        await page.keyboard.press('ArrowRight');
        await page.keyboard.press('ArrowRight');
        await expect(seek).toBeFocused();
        expect(Number(await seek.inputValue()) - before).toBe(200);
        await page.keyboard.press('Tab');
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => Math.round(element.currentTime * 1000))).toBe(Math.round(before + 200));
        const volume = region.getByRole('slider', { name: 'Volume', exact: true });
        await volume.focus();
        await page.keyboard.press('Home');
        await page.keyboard.press('ArrowUp');
        await expect(volume).toBeFocused();
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.volume)).toBe(0.05);
        expect(api.requests.filter((request) => request.path === '/playback/seek')).toHaveLength(0);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect.poll(() => playback.stops.length).toBe(1);
    });

    test('real heartbeat carries the playing session position before close', async ({ page, playback }) => {
        test.setTimeout(30_000);
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        await expectDecodedPlayback(page);
        await expect.poll(() => playback.heartbeats.length, { timeout: 17_000 }).toBeGreaterThan(0);
        expect(playback.heartbeats[0]).toMatchObject({ session_id: playback.starts[0].session_id, state: 'playing', is_buffering: false });
        expect(playback.heartbeats[0].position_ms).toBeGreaterThan(10_000);
        await region.getByRole('button', { name: 'Close player', exact: true }).hover();
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, movieUrl);
        await expect.poll(() => playback.stops.length).toBe(1);
        expect(playback.stops[0].session_id).toBe(playback.heartbeats[0].session_id);
    });

    test('focused native controls remain visible and operable through idle time', async ({ page, playback }) => {
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        await expectDecodedPlayback(page);
        const pause = region.getByRole('button', { name: 'Pause', exact: true });
        await pause.focus();
        await page.waitForTimeout(3500);
        await expect(pause).toBeFocused();
        expect(await pause.evaluate((button) => getComputedStyle(button.closest('[role="group"]')).opacity)).toBe('1');
        await page.keyboard.press('Enter');
        await expect(region.getByRole('button', { name: 'Play', exact: true })).toBeFocused();
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect.poll(() => playback.stops.length).toBe(1);
    });
});

test.describe('real HLS media', () => {
    test.use({ playbackOptions: { mode: 'transcode' } });

    test('decodes manifest and transport segments through hls.js', { tag: '@hls' }, async ({ page, api, playback }) => {
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        await expectDecodedPlayback(page);
        expect([...playback.sessions.values()][0].decision).toBe('transcode');
        expect(api.requests.some((request) => /\/transcode\/[^/]+\/manifest\.m3u8$/.test(request.path))).toBe(true);
        expect(api.requests.some((request) => /\/transcode\/[^/]+\/v0\/index\.m3u8$/.test(request.path))).toBe(true);
        expect(api.requests.some((request) => /\/transcode\/[^/]+\/v0\/segment-\d+\.ts$/.test(request.path))).toBe(true);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, movieUrl);
        await expect.poll(() => playback.stops.length).toBe(1);
    });
});

test('native fullscreen Escape leaves playback open; X exits and restores exact episode context', async ({ page, playback }) => {
    const region = await openPlayback(page, episodeUrl, mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
    await expect(region.getByRole('button', { name: 'Exit fullscreen', exact: true })).toBeVisible();
    await page.keyboard.press('Escape');
    await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(false);
    expect(new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
    expect(playback.stops).toHaveLength(0);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, episodeUrl);
    await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(false);
    await expect.poll(() => playback.stops.length).toBe(1);
    await expect(page.getByRole('heading', { name: 'Episode 1 · A New Arrival', exact: true })).toBeVisible();
});

test.describe('native media completion', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: false } });

    test('a real short movie raises trusted native ended without starting another item', async ({ page, playback }) => {
        const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        const completion = await video.evaluate((element: HTMLVideoElement) => ({ ended: element.ended, time: element.currentTime, duration: element.duration }));
        expect(completion.ended).toBe(true);
        expect(completion.time).toBeCloseTo(completion.duration, 1);
        expect(playback.starts).toHaveLength(1);
        await expect(page.getByRole('button', { name: /^(Play now|Play next)$/ })).toHaveCount(0);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, movieUrl);
        await expect.poll(() => playback.stops.length).toBe(1);
    });
});

test.describe('superseded playback start', () => {
    test.use({ playbackOptions: { startDelayMs: 1500 } });

    test('closing before start resolves stops the late session once without changing watch data', async ({ page, api, playback }) => {
        const region = await openPlayback(page, episodeUrl, mediaIds.episodeOne);
        await expect.poll(() => playback.starts.length).toBe(1);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl);
        await expect.poll(() => playback.stops.length).toBe(1);
        expect(playback.stops[0]).toMatchObject({ session_id: playback.starts[0].session_id, position_ms: 0, cancelled_before_start: true });
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne]).toMatchObject({ play_count: 0, resume_position_ms: 0 });
        expect(api.requests.filter((request) => request.path === '/playback/heartbeat')).toHaveLength(0);
        expect(api.requests.filter((request) => request.path === '/playback/qoe')).toHaveLength(0);
        expect(api.requests.filter((request) => request.path.startsWith('/stream/'))).toHaveLength(0);
        await expect(page.locator('video')).toHaveCount(0);
    });
});

test('playback start failure retains retry and the full Title close destination', async ({ page, api, playback }) => {
    api.failures.set('POST /playback/start', { status: 503, detail: 'Fixture playback start unavailable' });
    const region = await openPlayback(page, episodeUrl, mediaIds.episodeOne);
    await expect(region.getByText('Playback error. The stream may be unavailable.', { exact: true })).toBeVisible();
    expect(playback.starts).toHaveLength(0);
    api.failures.delete('POST /playback/start');
    await region.getByRole('button', { name: 'Retry', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expectDecodedPlayback(page);
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, episodeUrl);
    await expect.poll(() => playback.stops.length).toBe(1);
});
