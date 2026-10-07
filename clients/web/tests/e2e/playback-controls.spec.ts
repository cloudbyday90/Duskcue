import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const origin = '/media?type=series&sort=title&order=asc&watch=unwatched';
const episodeUrl = (episode = mediaIds.episodeOne) => `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${episode}&from=${encodeURIComponent(origin)}`;
const movieUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;

test('episode disclosure keyboard selection preserves fullscreen and the selected Title context', async ({ page, api, playback }) => {
    const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
    const trigger = region.getByRole('button', { name: 'Episodes', exact: true });
    await trigger.focus();
    await page.keyboard.press('Enter');
    await expect(trigger).toHaveAttribute('aria-expanded', 'true');
    const choices = region.getByRole('region', { name: 'Episodes', exact: true });
    await expect(choices.getByRole('button', { name: 'Episode 1 · A New Arrival', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.screenshot({ path: test.info().outputPath('player-fullscreen-episodes.png') });
    await page.keyboard.press('Escape');
    await expect(trigger).toHaveAttribute('aria-expanded', 'false');
    await expect(trigger).toBeFocused();
    expect(await page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
    await page.keyboard.press('Enter');
    const next = choices.getByRole('button', { name: 'Episode 2 · The Last Ferry', exact: true });
    await next.focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
    expect(playback.stops.filter((stop) => stop.session_id === playback.starts[0].session_id)).toHaveLength(1);
    expect(await page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
    await expect(trigger).toBeFocused();
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(1);
});

test('Settings changes native playback speed without restart and quality with a fresh session', async ({ page, playback }) => {
    const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
    const video = await expectDecodedPlayback(page);
    const trigger = region.getByRole('button', { name: 'Settings', exact: true });
    await trigger.click();
    const settings = region.getByRole('region', { name: 'Settings', exact: true });
    await settings.getByRole('button', { name: '2×', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.playbackRate)).toBe(2);
    expect(playback.starts).toHaveLength(1);
    await expect(trigger).toBeFocused();
    await page.keyboard.press('Enter');
    await settings.getByRole('button', { name: 'Up to 1.5 Mbps', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    expect(playback.starts[1]).toMatchObject({ media_item_id: mediaIds.resumeMovie, quality_mode: 'manual', max_streaming_bitrate: 1_500_000 });
    expect(playback.stops.filter((stop) => stop.session_id === playback.starts[0].session_id)).toHaveLength(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, movieUrl);
});

test('the real file audio stream appears as a labeled choice and maps its index to playback', async ({ page, api, playback }) => {
    api.files[mediaIds.resumeMovie][0].additional_streams = { audio: [{ index: 1, language: 'eng', codec: 'aac', channels: 2, disposition: { default: true } }], subtitles: [] };
    const region = await openPlayback(page, movieUrl, mediaIds.resumeMovie);
    await expectDecodedPlayback(page);
    const trigger = region.getByRole('button', { name: 'Audio & subtitles', exact: true });
    await trigger.focus();
    await page.keyboard.press('Enter');
    const tracks = region.getByRole('region', { name: 'Audio & subtitles', exact: true });
    await tracks.getByRole('button', { name: 'English', exact: true }).click();
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    expect(playback.starts[1].audio_stream_index).toBe(1);
    await expect(trigger).toBeFocused();
    await page.keyboard.press('Enter');
    await expect(tracks.getByRole('button', { name: 'English', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.keyboard.press('Escape');
    await expect(trigger).toBeFocused();
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
});

test.describe('narrow player disclosures', () => {
    test.use({ viewport: { width: 320, height: 740 }, scenarioOptions: { browsePageSize: 1 } });

    test('RTL reduced-motion menus remain inside the actual player viewport', async ({ page, playback }) => {
        await page.emulateMedia({ reducedMotion: 'reduce' });
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        await expectDecodedPlayback(page);
        await page.evaluate(() => { document.documentElement.dir = 'rtl'; });
        for (const name of ['Episodes', 'Settings']) {
            const trigger = region.getByRole('button', { name, exact: true });
            await trigger.click();
            const menu = region.getByRole('region', { name, exact: true });
            await expect(menu).toBeVisible();
            await expect.poll(async () => {
                const bounds = await menu.boundingBox();
                return !!bounds && bounds.x >= 0 && bounds.x + bounds.width <= 320 && bounds.y >= 0 && bounds.y + bounds.height <= 740;
            }).toBe(true);
            await page.screenshot({ path: test.info().outputPath(`player-rtl-320-${name.toLowerCase()}.png`) });
            await page.keyboard.press('Escape');
            await expect(trigger).toBeFocused();
        }
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect.poll(() => playback.stops.length).toBe(1);
    });
});

test.describe('trusted native episode ended with saved Off', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: false }, scenarioOptions: { browsePageSize: 1 } });

    test('Off remains untimed beyond ten seconds and Play next starts the exact next episode', async ({ page, api, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(next).toBeVisible();
        await expect(region.getByRole('button', { name: 'Cancel', exact: true })).toHaveCount(0);
        await page.clock.fastForward(11_000);
        expect(playback.starts).toHaveLength(1);
        await next.click();
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].is_watched).toBe(true);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });
});

test.describe('trusted native episode ended with saved On', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: true }, scenarioOptions: { browsePageSize: 1 } });

    test('Cancel retains an untimed Play next with focus and prevents the scheduled transition', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        await region.getByRole('button', { name: 'Cancel', exact: true }).focus();
        await page.keyboard.press('Enter');
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(next).toBeFocused();
        await page.screenshot({ path: test.info().outputPath('player-autoplay-cancelled.png') });
        await page.clock.fastForward(11_000);
        expect(playback.starts).toHaveLength(1);
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('the ten-second countdown starts next once and preserves actual fullscreen', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
        await expect.poll(() => page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        await page.clock.fastForward(9000);
        expect(playback.starts).toHaveLength(1);
        await page.clock.fastForward(1500);
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        expect(playback.stops.filter((stop) => stop.session_id === playback.starts[0].session_id)).toHaveLength(1);
        expect(await page.evaluate(() => document.fullscreenElement !== null)).toBe(true);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('a final episode ends without wrapping to the first episode or another season', async ({ page, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, episodeUrl(mediaIds.episodeTwo), mediaIds.episodeTwo);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        await expect(region.getByRole('button', { name: /^(Play now|Play next)$/ })).toHaveCount(0);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl(mediaIds.episodeTwo));
    });

    test('unavailable next episode shows retry and revalidates healthy files before untimed Play next', async ({ page, api, playback }) => {
        api.files[mediaIds.episodeTwo].forEach((file) => { file.is_healthy = false; });
        const region = await openPlayback(page, episodeUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('region', { name: 'The Last Ferry', exact: true }).getByText('The next episode is unavailable. Try again to check its availability.', { exact: true })).toBeVisible();
        await expect(region.getByRole('button', { name: /^(Play now|Play next)$/ })).toHaveCount(0);
        expect(playback.starts).toHaveLength(1);
        api.files[mediaIds.episodeTwo].forEach((file) => { file.is_healthy = true; });
        await region.getByRole('button', { name: 'Try again', exact: true }).click();
        const play = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(play).toBeFocused();
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
    });
});
