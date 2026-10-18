import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;
test.use({ playbackOptions: { directFallback: true } });

test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
        Object.defineProperty(navigator, 'mediaCapabilities', { configurable: true, value: {
            decodingInfo: async () => ({ supported: true, smooth: true, powerEfficient: false }),
        } });
    });
});

test('mocked native code3 and capability evidence adopt mock-API HLS under the existing session', async ({ page, playback }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    const id = playback.starts[0].session_id;
    const position = await video.evaluate((element: HTMLVideoElement) => {
        const position = element.currentTime;
        Object.defineProperty(element, 'error', { configurable: true, value: { code: 3 } });
        element.dispatchEvent(new Event('error'));
        delete (element as any).error;
        return position;
    });
    await expect.poll(() => playback.heartbeats.some((body) => body.recovery_requested && body.direct_fallback_supported)).toBe(true);
    const hint = playback.heartbeats.find((body) => body.recovery_requested);
    expect(hint.session_id).toBe(id);
    expect(hint.position_ms).toBeGreaterThanOrEqual(Math.floor(position * 1000));
    expect(hint.direct_source_revision).toBeTruthy();
    await expectDecodedPlayback(page);
    await expect(region.getByText('A more compatible playback stream is ready.', { exact: true })).toBeVisible();
    expect(playback.starts).toHaveLength(1);
    expect(playback.sessions.get(id).decision).toBe('transcode');
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
    expect(playback.stops).toHaveLength(1);
    expect(playback.stops[0].session_id).toBe(id);
});

test('a direct seek waits for mock-API revision ownership before changing native currentTime', async ({ page, playback }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    const before = await video.evaluate((element: HTMLVideoElement) => ({ time: element.currentTime, src: element.currentSrc }));
    const session = playback.sessions.get(playback.starts[0].session_id);
    const revision = session.directRevision;
    let release!: () => void;
    const held = new Promise<void>((resolve) => { release = resolve; });
    let requested = false;
    const seekRoute = '**/api/v1/playback/seek';
    await page.route(seekRoute, async (route) => { requested = true; await held; await route.fallback(); });
    try {
        await region.focus();
        await page.keyboard.press('ArrowRight');
        await expect.poll(() => requested).toBe(true);
        expect(await video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeCloseTo(before.time, 2);
        release();
        await expect.poll(() => playback.seeks.length).toBe(1);
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeCloseTo(playback.seeks[0].position_ms / 1000, 2);
        expect(session.directRevision).not.toBe(revision);
        expect(await video.evaluate((element: HTMLVideoElement) => ({ src: element.currentSrc, paused: element.paused }))).toEqual({ src: before.src, paused: true });
    } finally { release(); await page.unroute(seekRoute); }
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
});

test('Pause during a held mocked capability query remains paused after mock-API HLS adoption', async ({ page, playback }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    await video.evaluate((element: HTMLVideoElement) => {
        Object.defineProperty(navigator, 'mediaCapabilities', { configurable: true, value: {
            decodingInfo: () => new Promise((resolve) => { Reflect.set(window, 'fixtureReleaseCapability', () => resolve({ supported: true })); }),
        } });
        Object.defineProperty(element, 'error', { configurable: true, value: { code: 3 } });
        element.dispatchEvent(new Event('error'));
        delete (element as any).error;
    });
    await expect.poll(() => page.evaluate(() => typeof Reflect.get(window, 'fixtureReleaseCapability'))).toBe('function');
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    await expect(region.getByRole('button', { name: 'Play', exact: true })).toBeVisible();
    await page.evaluate(() => Reflect.get(window, 'fixtureReleaseCapability')());
    await expect.poll(() => playback.heartbeats.some((body) => body.direct_fallback_supported && body.is_paused === true)).toBe(true);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => ({ ready: element.readyState >= 2, width: element.videoWidth, paused: element.paused }))).toEqual({ ready: true, width: 320, paused: true });
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
});

test('a direct seek cancels a held mocked capability result and reattaches its detached native URL', async ({ page, playback }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    const originalUrl = await video.evaluate((element: HTMLVideoElement) => element.currentSrc);
    await video.evaluate((element: HTMLVideoElement) => {
        Object.defineProperty(navigator, 'mediaCapabilities', { configurable: true, value: {
            decodingInfo: () => new Promise((resolve) => { Reflect.set(window, 'fixtureReleaseCapability', () => resolve({ supported: true })); }),
        } });
        Object.defineProperty(element, 'error', { configurable: true, value: { code: 3 } });
        element.dispatchEvent(new Event('error'));
        delete (element as any).error;
    });
    await expect.poll(() => page.evaluate(() => typeof Reflect.get(window, 'fixtureReleaseCapability'))).toBe('function');
    await region.focus();
    await page.keyboard.press('ArrowRight');
    await expect.poll(() => playback.seeks.length).toBe(1);
    await expectDecodedPlayback(page);
    expect(await video.evaluate((element: HTMLVideoElement) => element.currentSrc)).toBe(originalUrl);
    await page.evaluate(() => Reflect.get(window, 'fixtureReleaseCapability')());
    expect(playback.heartbeats.filter((body) => body.direct_fallback_supported || body.recovery_requested)).toHaveLength(0);
    expect(playback.sessions.get(playback.starts[0].session_id).decision).toBe('direct_play');
    expect(playback.starts).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
});

for (const code of [2, 4]) {
    test(`mocked native error code${code} never sends a direct fallback allocation hint`, async ({ page, playback }) => {
        const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        await video.evaluate((element: HTMLVideoElement, code) => {
            Object.defineProperty(element, 'error', { configurable: true, value: { code } });
            element.dispatchEvent(new Event('error'));
            delete (element as any).error;
        }, code);
        await expect(region.getByRole('button', { name: 'Retry', exact: true })).toBeVisible();
        expect(playback.heartbeats.filter((body) => body.direct_fallback_supported || body.recovery_requested)).toHaveLength(0);
        expect(playback.sessions.get(playback.starts[0].session_id).decision).toBe('direct_play');
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
    });
}
