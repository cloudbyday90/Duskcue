import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const episodeUrl = (episode = mediaIds.episodeOne) => `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${episode}&from=${encodeURIComponent('/media?type=series')}`;

test.describe('real ended and controlled countdown scheduling', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: true }, scenarioOptions: { browsePageSize: 1 } });

    test('keyboard focus pauses countdown and the polite announcement stays stable between seconds', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        const play = region.getByRole('button', { name: 'Play now', exact: true });
        await expect(play).toBeVisible();
        await play.focus();
        await expect(region.getByText('Countdown paused at 10s', { exact: true })).toBeVisible();
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        await expect(play).toBeFocused();
        await region.focus();
        const status = region.getByRole('status').filter({ hasText: 'Automatic playback is on.' });
        await expect(status).toHaveCount(1);
        const announcement = await status.textContent();
        await page.clock.fastForward(4000);
        await expect(region.getByText('Playing next in 6s', { exact: true })).toBeVisible();
        expect(await status.textContent()).toBe(announcement);
        expect(playback.starts).toHaveLength(1);
        await page.clock.fastForward(6500);
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('an open disclosure pauses countdown until dismissed without moving the chosen focus', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        const settings = region.getByRole('button', { name: 'Settings', exact: true });
        await settings.click();
        await expect(settings).toHaveAttribute('aria-expanded', 'true');
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        await expect(region.getByText('Countdown paused at 10s', { exact: true })).toBeVisible();
        await page.keyboard.press('Escape');
        await expect(settings).toBeFocused();
        await page.clock.fastForward(10_500);
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
    });

    test('synthetic document visibility pauses scheduling after a trusted native ended event', async ({ page, playback }) => {
        await page.clock.install();
        await page.addInitScript(() => { Object.defineProperty(document, 'hidden', { configurable: true, get: () => true }); });
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByText('Countdown paused at 10s', { exact: true })).toBeVisible();
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        await page.evaluate(() => {
            Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
            document.dispatchEvent(new Event('visibilitychange'));
        });
        await expect(region.getByText('Playing next in 10s', { exact: true })).toBeVisible();
        await page.clock.fastForward(9000);
        expect(playback.starts).toHaveLength(1);
        await page.clock.fastForward(1500);
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('failed automatic start stays recoverable and retry offers only untimed exact-next playback', async ({ page, api, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        api.failures.set('POST /playback/start', { status: 503, detail: 'Fixture next episode unavailable' });
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        await page.clock.fastForward(10_500);
        const retry = region.getByRole('button', { name: 'Try again', exact: true });
        await expect(retry).toBeVisible();
        expect(playback.starts).toHaveLength(1);
        await page.clock.fastForward(15_000);
        expect(playback.starts).toHaveLength(1);
        api.failures.delete('POST /playback/start');
        await retry.focus();
        await page.keyboard.press('Enter');
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(next).toBeFocused();
        await page.clock.fastForward(15_000);
        expect(playback.starts).toHaveLength(1);
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        expect(playback.stops.filter((stop) => stop.session_id === playback.starts[0].session_id)).toHaveLength(1);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('seeking away from the real end clears countdown instead of starting another episode', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        await seek.focus();
        await page.keyboard.press('Home');
        await page.keyboard.press('Tab');
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toHaveCount(0);
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        expect(await video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
    });
});

test('saved track defaults and one-off overrides re-match actual next-file identities and retain speed volume and quality', async ({ page, api, playback }) => {
    api.preferencesByProfile[profileIds.alex].viewing_preferences = { autoplay_next_episode: false, audio_language: 'en', prefer_audio_description: true, subtitle_mode: 'always', subtitle_language: 'en', prefer_sdh: true };
    api.files[mediaIds.episodeOne][0].additional_streams = { audio: [{ index: 1, language: 'eng', codec: 'aac', disposition: { default: true, visual_impaired: false } }, { index: 2, language: 'eng', codec: 'aac', disposition: { visual_impaired: true } }], subtitles: [{ index: 3, language: 'eng', codec: 'subrip', disposition: { hearing_impaired: true } }] };
    api.files[mediaIds.episodeTwo][0].additional_streams = { audio: [{ index: 6, language: 'eng', codec: 'aac', disposition: { default: true, visual_impaired: false } }, { index: 8, language: 'eng', codec: 'aac', disposition: { visual_impaired: true } }], subtitles: [{ index: 9, language: 'eng', codec: 'subrip', disposition: { hearing_impaired: true } }] };
    const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    expect(playback.starts[0]).toMatchObject({ audio_stream_index: 2, subtitle_stream_index: 3, force_transcode: true });
    const volume = region.getByRole('slider', { name: 'Volume', exact: true });
    await volume.focus();
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowUp');
    const settings = region.getByRole('button', { name: 'Settings', exact: true });
    await settings.click();
    await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: '1.5×', exact: true }).click();
    await settings.click();
    await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Up to 3 Mbps', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    const tracks = region.getByRole('button', { name: 'Audio & subtitles', exact: true });
    await tracks.click();
    await region.getByRole('region', { name: 'Audio & subtitles', exact: true }).getByRole('button', { name: 'English', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(3);
    await expectDecodedPlayback(page);
    expect(playback.starts[2].audio_stream_index).toBe(1);
    await tracks.click();
    await region.getByRole('region', { name: 'Audio & subtitles', exact: true }).getByRole('button', { name: 'Off', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(4);
    await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Episodes', exact: true }).click();
    await region.getByRole('region', { name: 'Episodes', exact: true }).getByRole('button', { name: 'Episode 2 · The Last Ferry', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(5);
    await expectDecodedPlayback(page);
    expect(playback.starts[4]).toMatchObject({ media_item_id: mediaIds.episodeTwo, audio_stream_index: 6, subtitle_stream_index: null, quality_mode: 'manual', max_streaming_bitrate: 3_000_000 });
    expect(await video.evaluate((element: HTMLVideoElement) => ({ speed: element.playbackRate, volume: element.volume }))).toEqual({ speed: 1.5, volume: 0.05 });
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
});
