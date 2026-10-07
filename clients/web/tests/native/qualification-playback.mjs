import { expect } from '@playwright/test';
import { mediaIds, profileIds } from '../fixtures/catalog.js';
import { invokeNative } from './qualification-api.mjs';

const originContext = '/search?q=Harbor&type=series&sort=title&order=asc&watch=unwatched&pages=2';
const episodeTitle = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent(originContext)}`;

async function go(page, path) {
    await page.goto(`${new URL(page.url()).origin}${path}`);
}

async function assertDecoded(page) {
    const video = page.locator('video');
    await expect.poll(() => video.evaluate((element) => ({
        width: element.videoWidth, height: element.videoHeight, ready: element.readyState >= 2,
        advancing: element.currentTime > 0.2, paused: element.paused,
    })), { timeout: 15_000 }).toEqual({ width: 320, height: 180, ready: true, advancing: true, paused: false });
    return video;
}

async function assertTitle(page, expectedPath, episodeId = null) {
    const expected = new URL(expectedPath, 'http://fixture.test');
    await expect.poll(() => new URL(page.url()).pathname).toBe(expected.pathname);
    const params = new URLSearchParams(expected.search);
    if (episodeId) params.set('episode', episodeId);
    expect([...new URL(page.url()).searchParams].sort()).toEqual([...params].sort());
    await expect(page.getByRole('button', { name: /^(Play|Resume)$/ })).toBeEnabled();
}

export async function exercisePreferences(page, api, screenshot) {
    await go(page, '/settings/preferences');
    await expect(page.getByRole('group', { name: 'This profile', exact: true })).toBeVisible();
    await page.getByLabel('Play the next episode automatically', { exact: true }).uncheck();
    await page.getByLabel('Streaming quality', { exact: true }).selectOption('manual');
    await page.getByLabel('Bitrate limit (Mbps)', { exact: true }).selectOption('6000000');
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
    await page.reload();
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).not.toBeChecked();
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('manual');
    await expect(page.getByLabel('Bitrate limit (Mbps)', { exact: true })).toHaveValue('6000000');
    expect(api.scenario.preferencesByProfile[profileIds.alex].viewing_preferences.autoplay_next_episode).toBe(false);
    await screenshot('native-preferences');
    return { savedOff: true, savedDeviceQuality: true, reloaded: true };
}

export async function exerciseNativePlayback(page, runtime, api, screenshot) {
    const playback = api.getPlayback();
    await go(page, episodeTitle);
    await expect(page.getByRole('button', { name: /^(Play|Resume)$/ })).toBeEnabled();
    await expect(page.getByRole('heading', { name: 'The Last Ferry', exact: true })).toBeVisible();
    await screenshot('native-episode-gallery');
    await page.getByRole('button', { name: /^(Play|Resume)$/ }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    const video = await assertDecoded(page);
    await video.evaluate((element) => { element.dataset.qualificationContinuity = 'original-native-video'; });
    expect(playback.starts[0]).toMatchObject({ media_item_id: mediaIds.episodeOne, quality_mode: 'manual', max_streaming_bitrate: 6_000_000, force_transcode: true });
    await region.getByRole('slider', { name: 'Volume', exact: true }).focus();
    await page.keyboard.press('Home');
    for (let index = 0; index < 8; index += 1) await page.keyboard.press('ArrowRight');
    await expect.poll(() => video.evaluate((element) => element.volume)).toBeCloseTo(0.4, 3);
    await region.getByRole('button', { name: 'Settings', exact: true }).click();
    await region.getByRole('button', { name: '1.5×', exact: true }).click();
    await expect(region.getByRole('button', { name: 'Settings', exact: true })).toHaveAttribute('aria-expanded', 'false');
    await expect(region.getByRole('button', { name: 'Settings', exact: true })).toBeFocused();
    await expect.poll(() => video.evaluate((element) => element.playbackRate)).toBe(1.5);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect(region.getByRole('button', { name: 'Exit fullscreen', exact: true })).toBeVisible();
    const browserFullscreen = await page.evaluate(() => document.fullscreenElement !== null);
    const normalNativeFullscreen = await invokeNative(page, 'plugin:window|is_fullscreen', { label: 'main' });
    expect(browserFullscreen || normalNativeFullscreen).toBe(true);
    await page.keyboard.press('Escape');
    await expect(region.getByRole('button', { name: 'Fullscreen', exact: true })).toBeVisible();
    expect(new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);

    await region.evaluate((element) => {
        Object.defineProperty(element, 'requestFullscreen', { configurable: true, value: () => Promise.reject(new DOMException('Controlled browser fullscreen failure', 'NotAllowedError')) });
    });
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => invokeNative(page, 'plugin:window|is_fullscreen', { label: 'main' })).toBe(true);
    expect(await page.evaluate(() => document.fullscreenElement !== null)).toBe(false);
    await region.getByRole('button', { name: 'Settings', exact: true }).click();
    await page.keyboard.press('Escape');
    await expect(region.getByRole('button', { name: 'Settings', exact: true })).toHaveAttribute('aria-expanded', 'false');
    await expect(region.getByRole('button', { name: 'Settings', exact: true })).toBeFocused();
    expect(await invokeNative(page, 'plugin:window|is_fullscreen', { label: 'main' })).toBe(true);
    await screenshot('native-fullscreen-fallback');
    await page.evaluate(() => {
        document.addEventListener('ended', (event) => {
            if (event.isTrusted && event.target instanceof HTMLVideoElement) event.target.dataset.nativeEnded = 'true';
        }, { capture: true, once: true });
    });
    await video.evaluate((element) => { element.currentTime = element.duration - 0.4; });
    await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
    await expect(region.getByRole('button', { name: 'Play next', exact: true })).toBeVisible();
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toHaveCount(0);
    expect(playback.starts).toHaveLength(1);
    await screenshot('native-autoplay-off');
    await region.getByRole('button', { name: 'Play next', exact: true }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeTwo}`);
    await assertDecoded(page);
    await expect(video).toHaveAttribute('data-qualification-continuity', 'original-native-video');
    const authenticatedMedia = api.transportRequests.filter((request) => /\/transcode\/[^/]+\//.test(request.path));
    expect(authenticatedMedia.length).toBeGreaterThanOrEqual(3);
    expect(authenticatedMedia.every((request) => request.authenticated)).toBe(true);
    expect(playback.starts).toHaveLength(2);
    expect(playback.starts[1]).toMatchObject({ media_item_id: mediaIds.episodeTwo, quality_mode: 'manual', max_streaming_bitrate: 6_000_000 });
    expect(await video.evaluate((element) => element.playbackRate)).toBe(1.5);
    expect(await video.evaluate((element) => element.volume)).toBeCloseTo(0.4, 3);
    expect(await invokeNative(page, 'plugin:window|is_fullscreen', { label: 'main' })).toBe(true);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    const finalPosition = await video.evaluate((element) => element.currentTime * 1000);
    const watchPath = `/items/${mediaIds.episodeTwo}/watch-data`;
    const priorWatchReads = api.scenario.requests.filter((request) => request.method === 'GET' && request.path === watchPath).length;
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await assertTitle(page, episodeTitle, mediaIds.episodeTwo);
    await expect.poll(() => playback.stops.length).toBe(2);
    expect(new Set(playback.stops.map((stop) => stop.session_id)).size).toBe(2);
    expect(playback.stops[1].position_ms).toBeGreaterThanOrEqual(Math.floor(finalPosition) - 300);
    expect(api.scenario.watchByProfile[profileIds.alex][mediaIds.episodeTwo].resume_position_ms).toBe(playback.stops[1].position_ms);
    expect(api.scenario.requests.filter((request) => request.method === 'GET' && request.path === watchPath).length).toBeGreaterThan(priorWatchReads);
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    expect(await invokeNative(page, 'plugin:window|is_fullscreen', { label: 'main' })).toBe(false);
    expect(runtime.child.exitCode).toBeNull();
    expect(await invokeNative(page, 'app_info')).toMatchObject({ name: 'Duskcue' });
    await screenshot('native-title-after-close');
    return { decodedAuthenticatedHls: true, forcedAuthenticatedTransport: true, browserFullscreen, normalNativeFullscreen, controlledBrowserFailureNativeFallback: true, menuEscapeRetainedNativeFullscreen: true, actualTrustedEnded: true, savedOffUntimed: true, manualNextRetainedQualitySpeedVolumeFullscreenVideoNode: true, closeReturnedCanonicalTitle: true, stoppedEachSessionOnce: true, applicationStayedOpen: true };
}

export async function exerciseNativeHls(page, api, screenshot) {
    const playback = api.getPlayback();
    const startsBefore = playback.starts.length;
    const stopsBefore = playback.stops.length;
    const requestOffset = api.scenario.requests.length;
    const transportOffset = api.transportRequests.length;
    const title = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie&sort=title&order=asc')}`;
    await go(page, title);
    await page.getByRole('button', { name: /^(Play|Resume)$/ }).click();
    await assertDecoded(page);
    await screenshot('native-hls-player');
    expect(playback.starts.length).toBe(startsBefore + 1);
    const start = playback.starts[startsBefore];
    expect(start).toMatchObject({ media_item_id: mediaIds.resumeMovie, force_transcode: true });
    const prefix = `/transcode/${playback.sessions.get(start.session_id).transcodeId}/`;
    const phaseRequests = api.scenario.requests.slice(requestOffset);
    expect(phaseRequests.some((request) => request.path === `${prefix}manifest.m3u8`)).toBe(true);
    expect(phaseRequests.some((request) => request.path.startsWith(prefix) && /\/segment-\d+\.ts$/.test(request.path))).toBe(true);
    const phaseTransport = api.transportRequests.slice(transportOffset).filter((request) => request.path.startsWith(prefix));
    expect(phaseTransport.length).toBeGreaterThanOrEqual(3);
    expect(phaseTransport.every((request) => request.authenticated && request.origin === api.authRequests.at(-1).origin)).toBe(true);
    await page.getByRole('button', { name: 'Close player', exact: true }).click();
    await assertTitle(page, title);
    await expect.poll(() => playback.stops.length).toBe(stopsBefore + 1);
    expect(playback.stops[stopsBefore].session_id).toBe(start.session_id);
    return { decodedRealHls: true, manifestAndSegmentsRequested: true, returnedMovieTitle: true };
}

export async function releaseNativePlaybackForCleanup(page, api) {
    const playback = api.getPlayback();
    const close = page.getByRole('button', { name: 'Close player', exact: true });
    const requestedClose = await close.count() === 1;
    if (requestedClose) {
        await close.click({ timeout: 5_000 });
        await expect.poll(() => new URL(page.url()).pathname.startsWith('/play/'), { timeout: 10_000 }).toBe(false);
    }
    const unreleasedSessions = [...playback.sessions.values()].filter((session) => !session.stopped).map((session) => session.id);
    if (unreleasedSessions.length) throw new Error(`Native fixture sessions remain unreleased: ${unreleasedSessions.join(', ')}.`);
    return { requestedClose, unreleasedSessions };
}
