import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;

test.describe('retained segment and storyboard behavior', () => {
    test.use({ playbackOptions: { segments: [{ id: '00000000-0000-7000-8000-000000000800', segment_type: 'intro', start_ms: 0, end_ms: 5000, skip_to_ms: 6000, is_manual: true, confidence: 1 }] } });

    test('manual intro skip seeks real media and keyboard seek loads an actual storyboard sprite', async ({ page, api, playback }) => {
        const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
        const video = await expectDecodedPlayback(page);
        const skip = region.getByRole('button', { name: 'Skip Intro', exact: true });
        await skip.focus();
        await page.keyboard.press('Enter');
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeGreaterThanOrEqual(6);
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        await seek.focus();
        await page.keyboard.press('ArrowRight');
        await expect.poll(() => api.requests.some((request) => request.path.endsWith('/storyboard/index.vtt'))).toBe(true);
        await expect.poll(() => api.requests.some((request) => request.path.endsWith('/storyboard/sprite-000.webp'))).toBe(true);
        await expect.poll(() => region.locator('.preview-thumbnail').evaluateAll((elements) => elements.some((element) => getComputedStyle(element).backgroundImage.startsWith('url("blob:')))).toBe(true);
        await page.keyboard.press('Tab');
        expect(playback.starts).toHaveLength(1);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, titleUrl);
        expect(playback.stops[0].position_ms).toBeGreaterThanOrEqual(6000);
    });
});

test.describe('retained HLS seek replacement', () => {
    test.use({ playbackOptions: { mode: 'transcode', seekReplacement: true } });

    test('server seek installs the new real HLS manifest while preserving the playback session and close position', async ({ page, api, playback }) => {
        const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
        await expectDecodedPlayback(page);
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        await seek.focus();
        const before = Number(await seek.inputValue());
        await page.keyboard.press('ArrowRight');
        await page.keyboard.press('Tab');
        await expect.poll(() => playback.seeks.length).toBe(1);
        expect(playback.seeks[0]).toMatchObject({ session_id: playback.starts[0].session_id, position_ms: Math.round(before + 100) });
        const replacement = [...playback.sessions.values()][0].transcodeId;
        await expect.poll(() => api.requests.some((request) => request.path === `/transcode/${replacement}/manifest.m3u8`)).toBe(true);
        await expectDecodedPlayback(page);
        expect(playback.starts).toHaveLength(1);
        expect(playback.stops).toHaveLength(0);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, titleUrl);
        expect(playback.stops[0]).toMatchObject({ session_id: playback.starts[0].session_id });
        expect(playback.stops[0].position_ms).toBeGreaterThanOrEqual(Math.round(before + 100));
    });
});
