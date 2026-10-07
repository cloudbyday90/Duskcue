import type { Locator, Page } from '@playwright/test';
import { test, expect } from './caption-fixtures';
import { expectDecodedPlayback, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}`;

test.skip(!process.env.DUSKCUE_TEST_CAPTION_HLS_DIR, 'Real-caption qualification is pending until an admitted production-argument HLS artifact is explicitly supplied.');
test.use({ playbackOptions: { mode: 'transcode', autoplay: true } });

async function captionEvidence(page: Page, video: Locator, label: string) {
    await page.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
    const evidence = await video.evaluate((element: HTMLVideoElement) => {
        const canvas = document.createElement('canvas');
        canvas.width = element.videoWidth;
        canvas.height = element.videoHeight;
        const context = canvas.getContext('2d', { willReadFrequently: true })!;
        context.drawImage(element, 0, 0);
        const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
        const bounds = element.getBoundingClientRect();
        const scale = Math.min(bounds.width / canvas.width, bounds.height / canvas.height);
        const left = bounds.left + (bounds.width - canvas.width * scale) / 2;
        const top = bounds.top + (bounds.height - canvas.height * scale) / 2;
        let decoded = 0;
        let visible = 0;
        let uncovered = 0;
        let minX = Infinity, maxX = 0, minY = Infinity, maxY = 0;
        const obstructions = new Set<string>();
        for (let y = Math.floor(canvas.height * 0.75); y < canvas.height; y += 1) {
            for (let x = 0; x < canvas.width; x += 1) {
                const index = (y * canvas.width + x) * 4;
                if (pixels[index] < 130 || pixels[index + 1] < 130 || pixels[index + 2] < 130) continue;
                decoded += 1;
                minX = Math.min(minX, x); maxX = Math.max(maxX, x); minY = Math.min(minY, y); maxY = Math.max(maxY, y);
                const screenX = left + (x + 0.5) * scale;
                const screenY = top + (y + 0.5) * scale;
                if (screenX < 0 || screenX >= innerWidth || screenY < 0 || screenY >= innerHeight) continue;
                visible += 1;
                const hit = document.elementFromPoint(screenX, screenY);
                if (hit === element) uncovered += 1;
                else if (hit) obstructions.add(`${hit.tagName}.${hit.className}`);
            }
        }
        return { decoded, visible, uncovered, obstructions: [...obstructions], caption: { minX, maxX, minY, maxY, scale }, video: { left: bounds.left, top: bounds.top, width: bounds.width, height: bounds.height }, viewport: { width: innerWidth, height: innerHeight }, position: element.currentTime, nativeEnded: element.dataset.nativeEnded === 'true' };
    });
    await test.info().attach(`${label}-caption-geometry`, { body: JSON.stringify(evidence, null, 2), contentType: 'application/json' });
    await page.screenshot({ path: test.info().outputPath(`${label}.png`) });
    expect(evidence.decoded, 'The actual decoded red-background stream must contain rendered caption pixels').toBeGreaterThan(50);
    expect(evidence.visible, 'Every rendered caption pixel must remain in the viewport').toBe(evidence.decoded);
    expect(evidence.uncovered, `Authored controls must not cover caption pixels: ${evidence.obstructions.join(', ')}`).toBe(evidence.decoded);
    return evidence;
}

test('actual burned captions remain unobscured while transport and each vertical disclosure are visible', async ({ page, playback, captions }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeGreaterThan(2);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    const originalSource = await video.evaluate((element: HTMLVideoElement) => { element.dataset.captionFixtureNode = 'original'; return element.currentSrc; });
    expect(playback.starts[0].subtitle_stream_index).toBe(4);
    await test.info().attach('actual-production-caption-source', { body: JSON.stringify(captions), contentType: 'application/json' });
    await captionEvidence(page, video, 'captions-controls');
    for (const name of ['Episodes', 'Audio & subtitles', 'Settings']) {
        const trigger = region.getByRole('button', { name, exact: true });
        await trigger.focus();
        await page.keyboard.press('Enter');
        await expect(region.getByRole('region', { name, exact: true })).toBeVisible();
        await captionEvidence(page, video, `captions-${name.replaceAll(/\W/g, '-').toLowerCase()}`);
        await page.keyboard.press('Escape');
        await expect(trigger).toBeFocused();
    }
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => video.evaluate((element) => document.fullscreenElement?.contains(element))).toBe(true);
    await captionEvidence(page, video, 'captions-fullscreen');
    await region.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
    await expect.poll(() => page.evaluate(() => document.fullscreenElement === null)).toBe(true);
    expect(await video.evaluate((element: HTMLVideoElement) => element.currentSrc)).toBe(originalSource);
    await expect(video).toHaveAttribute('data-caption-fixture-node', 'original');
    expect(playback.starts).toHaveLength(1);
    expect(playback.stops).toHaveLength(0);
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    await region.focus();
    await page.mouse.move(1, 1);
    await expect(region.locator('.player-controls')).toHaveCSS('opacity', '0');
    await captionEvidence(page, video, 'captions-hidden-controls');
    expect(await video.evaluate((element) => { const video = element.getBoundingClientRect(); const stage = element.parentElement!.getBoundingClientRect(); return { width: video.width === stage.width, height: video.height === stage.height }; })).toEqual({ width: true, height: true });
});

test('short-height disclosures keep the actual burned caption band visible and uncovered', async ({ page, captions }) => {
    await page.setViewportSize({ width: 640, height: 420 });
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.currentTime)).toBeGreaterThan(2);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    await test.info().attach('actual-production-caption-source', { body: JSON.stringify(captions), contentType: 'application/json' });
    await captionEvidence(page, video, 'captions-short-controls');
    const settings = region.getByRole('button', { name: 'Settings', exact: true });
    await settings.focus();
    await page.keyboard.press('Enter');
    await captionEvidence(page, video, 'captions-short-settings');
    await page.keyboard.press('Tab');
    const choice = region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Auto', exact: true });
    await expect(choice).toBeFocused();
    const target = await choice.evaluate((element) => { const box = element.getBoundingClientRect(); return { width: box.width, height: box.height, visible: box.top >= 0 && box.bottom <= innerHeight && box.left >= 0 && box.right <= innerWidth, hit: element.contains(document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2)) }; });
    expect(target.width).toBeGreaterThanOrEqual(44);
    expect(target.height).toBeGreaterThanOrEqual(44);
    expect(target.visible && target.hit).toBe(true);
    await captionEvidence(page, video, 'captions-short-focused-choice');
    await page.keyboard.press('Escape');
    await expect(settings).toBeFocused();
});

test.describe('native completion during the actual burned cue', () => {
    test.use({ captionOptions: { endDuringCue: true } });

    test('the frozen caption frame remains unobscured beneath Up next and Cancel keeps it visible', async ({ page, captions }) => {
        const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
        const play = region.getByRole('button', { name: 'Play now', exact: true });
        await play.focus();
        await test.info().attach('actual-production-caption-source', { body: JSON.stringify(captions), contentType: 'application/json' });
        await captionEvidence(page, video, 'captions-up-next');
        await region.getByRole('button', { name: 'Cancel', exact: true }).click();
        await expect(region.getByRole('button', { name: 'Play next', exact: true })).toBeFocused();
        await captionEvidence(page, video, 'captions-cancelled-up-next');
    });
});
