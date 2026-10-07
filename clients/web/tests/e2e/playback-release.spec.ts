import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const titleUrl = (episode = mediaIds.episodeOne) => `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${episode}&from=${encodeURIComponent('/media?type=series')}`;
const closingCopy = 'Could not finish closing playback. Try again.';

test('failed X release keeps a focused explicit retry and never navigates or sends another stop automatically', async ({ page, api, playback }) => {
    const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    api.failures.set('POST /playback/stop', { status: 503, detail: 'Fixture stop unavailable' });
    await region.getByRole('button', { name: 'Close player', exact: true }).focus();
    await page.keyboard.press('Enter');
    const retry = region.getByRole('button', { name: 'Retry closing playback', exact: true });
    await expect(retry).toBeFocused();
    await expect(region.getByRole('status').filter({ hasText: closingCopy })).toHaveText(closingCopy);
    expect(new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
    expect(await page.locator('video').evaluate((element: HTMLVideoElement) => ({ paused: element.paused, source: element.getAttribute('src') }))).toEqual({ paused: true, source: null });
    expect(playback.stopAttempts).toHaveLength(1);
    expect(playback.stops).toHaveLength(0);
    expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(0);
    await page.clock.install();
    await page.clock.fastForward(16000);
    expect(playback.stopAttempts).toHaveLength(1);
    api.failures.delete('POST /playback/stop');
    await page.keyboard.press('Enter');
    await expectTitleContext(page, titleUrl());
    expect(playback.stopAttempts).toHaveLength(2);
    expect(playback.stopAttempts[1]).toEqual(playback.stopAttempts[0]);
    expect(playback.stops).toHaveLength(1);
    expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(1);
});

test.describe('lost acknowledgement after a committed stop', () => {
    test.use({ playbackOptions: { lostStopAcknowledgements: 1 } });

    test('explicit X retry replays the committed final body without incrementing watch count again', async ({ page, api, playback }) => {
        const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
        await expectDecodedPlayback(page);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        const retry = region.getByRole('button', { name: 'Retry closing playback', exact: true });
        await expect(retry).toBeFocused();
        expect(playback.stopAttempts).toHaveLength(1);
        expect(playback.stops).toHaveLength(1);
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(1);
        await page.keyboard.press('Enter');
        await expectTitleContext(page, titleUrl());
        expect(playback.stopAttempts).toHaveLength(2);
        expect(playback.stopAttempts[1]).toEqual(playback.stopAttempts[0]);
        expect(playback.stops).toHaveLength(1);
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(1);
    });
});

test('failed browser Back release retains the original destination and history state until explicit retry', async ({ page, api, playback }) => {
    await page.goto('/media?type=series&sort=title&order=asc');
    await expect(page.getByRole('link', { name: /^Harbor Stories/ })).toBeVisible();
    await page.evaluate(() => { history.replaceState({ ...history.state, fixtureBackState: 'original-gallery' }, '', location.href); });
    await page.getByRole('link', { name: /^Harbor Stories/ }).click();
    await page.getByRole('button', { name: /^(Play|Resume)$/ }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    await expectDecodedPlayback(page);
    api.failures.set('POST /playback/stop', { status: 503, detail: 'Fixture stop unavailable' });
    await page.evaluate(() => history.go(-2));
    const retry = region.getByRole('button', { name: 'Retry closing playback', exact: true });
    await expect(retry).toBeVisible();
    await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
    expect(playback.stopAttempts).toHaveLength(1);
    api.failures.delete('POST /playback/stop');
    await retry.focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => new URL(page.url()).pathname).toBe('/media');
    expect(new URL(page.url()).search).toBe('?type=series&sort=title&order=asc');
    expect(await page.evaluate(() => history.state.fixtureBackState)).toBe('original-gallery');
    expect(playback.stopAttempts[1]).toEqual(playback.stopAttempts[0]);
});

test.describe('failed old-session release before next playback', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: true }, scenarioOptions: { browsePageSize: 1 } });

    test('keeps next untimed and does not start or skip another item until explicit release retry succeeds', async ({ page, api, playback }) => {
        await page.clock.install();
        const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        api.failures.set('POST /playback/stop', { status: 503, detail: 'Fixture previous-session stop unavailable' });
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
        await page.clock.fastForward(10500);
        const retry = region.getByRole('button', { name: 'Try again', exact: true });
        await expect(retry).toBeVisible();
        expect(playback.starts).toHaveLength(1);
        expect(playback.stopAttempts).toHaveLength(1);
        await page.clock.fastForward(15000);
        expect(playback.stopAttempts).toHaveLength(1);
        await retry.click();
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(next).toBeFocused();
        await page.clock.fastForward(15000);
        expect(playback.starts).toHaveLength(1);
        expect(playback.stopAttempts).toHaveLength(1);
        api.failures.delete('POST /playback/stop');
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        expect(playback.starts[1].media_item_id).toBe(mediaIds.episodeTwo);
        expect(playback.stopAttempts[1]).toEqual(playback.stopAttempts[0]);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, titleUrl(mediaIds.episodeTwo));
    });
});
