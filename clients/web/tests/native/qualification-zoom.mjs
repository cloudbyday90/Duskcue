import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { mediaIds } from '../fixtures/catalog.js';
import { invokeNative } from './qualification-api.mjs';
import { keyboardReach } from './qualification-focus.mjs';
import { exerciseDiscardZoom, exerciseProfileZoom } from './qualification-profile-zoom.mjs';
import { captureNativeArtifact, createNativeCaptureInventory } from './qualification-capture.mjs';
import { exerciseNativeBrowsing } from './qualification-browsing.mjs';

async function viewport(page) {
    return page.evaluate(() => ({
        width: innerWidth, height: innerHeight, devicePixelRatio,
        clientWidth: document.documentElement.clientWidth,
        scrollWidth: document.documentElement.scrollWidth,
        scrollHeight: document.documentElement.scrollHeight,
        visualScale: visualViewport?.scale ?? null,
        playerLayout: Object.fromEntries(['.player-container', '.player-heading', '.player-controls', '.autoplay-position', '.player-popover:not([hidden])'].map((selector) => {
            const element = document.querySelector(selector);
            if (!element) return [selector, null];
            const rect = element.getBoundingClientRect();
            const style = getComputedStyle(element);
            return [selector, { x: rect.x, y: rect.y, width: rect.width, height: rect.height, scrollWidth: element.scrollWidth, clientWidth: element.clientWidth, scrollHeight: element.scrollHeight, clientHeight: element.clientHeight, maxHeight: style.maxHeight, bottom: style.bottom, overflowX: style.overflowX, overflowY: style.overflowY }];
        })),
    }));
}

async function setZoom(page, value) {
    await invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value });
    await page.evaluate(() => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve))));
}

export async function exerciseNativeZoom(page, manifest, api, inventory = createNativeCaptureInventory()) {
    const evidence = { status: 'in_progress', method: 'Tauri 2.11.1 setZoom → actual WebView2 engine zoom', requestedFactor: 4, viewports: {}, focus: {}, contrast: inventory.contrast, screenshots: [], restored: false };
    const appOrigin = new URL(page.url()).origin;
    const capture = async (name) => {
        const path = await captureNativeArtifact(page, manifest, inventory, name, false);
        evidence.screenshots.push(path);
    };
    const record = async (name, reflow = true) => {
        const value = await viewport(page);
        evidence.viewports[name] = value;
        expect(Math.abs(value.width - evidence.baseline.width / 4), `${name} must keep the actual engine zoom across navigation`).toBeLessThanOrEqual(1);
        expect(Math.abs(value.height - evidence.baseline.height / 4)).toBeLessThanOrEqual(1);
        if (reflow) expect(value.scrollWidth, `${name} must not require horizontal document scrolling`).toBeLessThanOrEqual(value.clientWidth + 1);
        await capture(`native-400-${name}`);
        return value;
    };
    try {
        await setZoom(page, 1);
        await page.goto(`${appOrigin}/dashboard`);
        await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
        evidence.baseline = await viewport(page);
        evidence.nativeSizeBefore = await invokeNative(page, 'plugin:window|inner_size', { label: 'main' });
        await setZoom(page, 4);
        await expect.poll(async () => Math.abs((await viewport(page)).width - evidence.baseline.width / 4)).toBeLessThanOrEqual(1);
        const zoomed = await record('home');
        expect(zoomed.width, 'The 400% native viewport must be 320 CSS pixels or narrower').toBeLessThanOrEqual(320);
        expect(zoomed.devicePixelRatio / evidence.baseline.devicePixelRatio).toBeCloseTo(4, 2);
        evidence.nativeSizeAfter = await invokeNative(page, 'plugin:window|inner_size', { label: 'main' });
        expect(evidence.nativeSizeAfter).toEqual(evidence.nativeSizeBefore);
        await keyboardReach(page, page.getByRole('button', { name: 'Search', exact: true }), 'home-search-submit', evidence);
        await keyboardReach(page, page.getByRole('searchbox', { name: 'Search', exact: true }), 'home-search', evidence);
        evidence.browsing = await exerciseNativeBrowsing(page, api, (name) => record(name.replace(/^native-/, '')));
        evidence.profiles = await exerciseProfileZoom({ page, api, evidence, record, capture });

        await page.goto(`${appOrigin}/settings/preferences`);
        await expect(page.getByLabel('Play the next episode automatically', { exact: true })).not.toBeChecked();
        await record('preferences');
        await keyboardReach(page, page.getByLabel('Preferred audio language', { exact: true }), 'preference-language', evidence);
        await capture('native-400-preference-focus');
        evidence.discardDialog = await exerciseDiscardZoom({ page, evidence, record, capture });

        const titlePath = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent('/search?q=Harbor&type=series')}`;
        await page.goto(`${appOrigin}${titlePath}`);
        const play = page.getByRole('button', { name: /^(Play|Resume)$/ });
        await expect(play).toBeEnabled();
        await record('title');
        await keyboardReach(page, play, 'title-play', evidence);
        await capture('native-400-title-focus');
        const playback = api.getPlayback();
        const stopCount = playback.stops.length;
        await page.keyboard.press('Enter');
        await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
        const region = page.getByRole('region', { name: 'Media player', exact: true });
        const video = page.locator('video');
        await expect.poll(() => video.evaluate((element) => element.videoWidth > 0 && element.currentTime > 0.2 && !element.paused), { timeout: 15_000 }).toBe(true);
        await record('player', false);
        await keyboardReach(page, region.getByRole('button', { name: 'Close player', exact: true }), 'player-close', evidence);
        await capture('native-400-player-close-focus');
        await keyboardReach(page, region.getByRole('slider', { name: 'Seek', exact: true }), 'player-seek', evidence);
        await keyboardReach(page, region.getByRole('button', { name: 'Pause', exact: true }), 'player-pause', evidence);
        await page.keyboard.press('Enter');
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(true);
        await keyboardReach(page, region.getByRole('button', { name: 'Mute', exact: true }), 'player-mute', evidence);
        await keyboardReach(page, region.getByRole('slider', { name: 'Volume', exact: true }), 'player-volume', evidence);
        const episodes = region.getByRole('button', { name: 'Episodes', exact: true });
        await keyboardReach(page, episodes, 'player-episodes', evidence);
        await page.keyboard.press('Enter');
        await expect(episodes).toHaveAttribute('aria-expanded', 'true');
        await keyboardReach(page, region.getByRole('button', { name: /The Last Ferry$/ }), 'player-episode-choice', evidence);
        await capture('native-400-player-episodes-focus');
        await page.keyboard.press('Escape');
        await expect(episodes).toBeFocused();
        const tracks = region.getByRole('button', { name: 'Audio & subtitles', exact: true });
        await keyboardReach(page, tracks, 'player-tracks', evidence);
        await page.keyboard.press('Enter');
        await expect(tracks).toHaveAttribute('aria-expanded', 'true');
        await keyboardReach(page, region.getByRole('button', { name: 'Source default', exact: true }), 'player-audio-choice', evidence);
        await capture('native-400-player-tracks-focus');
        await page.keyboard.press('Escape');
        await expect(tracks).toBeFocused();
        await keyboardReach(page, region.getByRole('button', { name: 'Fullscreen', exact: true }), 'player-fullscreen', evidence);
        const settings = region.getByRole('button', { name: 'Settings', exact: true });
        await keyboardReach(page, settings, 'player-settings', evidence);
        await page.keyboard.press('Enter');
        await expect(settings).toHaveAttribute('aria-expanded', 'true');
        await keyboardReach(page, region.getByRole('button', { name: 'Up to 40 Mbps', exact: true }), 'player-popover-quality', evidence);
        await keyboardReach(page, region.getByRole('button', { name: '2×', exact: true }), 'player-popover-speed', evidence);
        await capture('native-400-player-settings-focus');
        await page.keyboard.press('Enter');
        await expect(settings).toHaveAttribute('aria-expanded', 'false');
        await expect(settings).toBeFocused();
        await keyboardReach(page, region.getByRole('button', { name: 'Play', exact: true }), 'player-play', evidence);
        await page.keyboard.press('Enter');
        await expect.poll(() => video.evaluate((element) => element.paused)).toBe(false);

        await page.evaluate(() => {
            document.addEventListener('ended', (event) => {
                if (event.isTrusted && event.target instanceof HTMLVideoElement) event.target.dataset.qualificationZoomEnded = 'true';
            }, { capture: true, once: true });
        });
        await video.evaluate((element) => { element.currentTime = Math.max(0, element.duration - 0.5); if (element.paused) element.play(); });
        await expect(video).toHaveAttribute('data-qualification-zoom-ended', 'true', { timeout: 10_000 });
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(next).toBeVisible();
        await keyboardReach(page, next, 'player-untimed-next', evidence);
        await record('up-next', false);
        await keyboardReach(page, region.getByRole('button', { name: 'Close player', exact: true }), 'player-close-after-ended', evidence);
        await page.keyboard.press('Enter');
        await expect.poll(() => new URL(page.url()).pathname).toBe(`/media/${mediaIds.series}`);
        expect(new URL(page.url()).searchParams.get('season')).toBe(mediaIds.seasonOne);
        expect(new URL(page.url()).searchParams.get('episode')).toBe(mediaIds.episodeOne);
        await expect.poll(() => playback.stops.length).toBe(stopCount + 1);
        await record('title-after-close');
        evidence.status = 'passed';
        return evidence;
    } catch (error) {
        evidence.status = 'failed';
        evidence.error = error.stack || String(error);
        await capture('native-400-failure').catch(() => {});
        throw error;
    } finally {
        try {
            await setZoom(page, 1);
            if (evidence.baseline) await expect.poll(async () => Math.abs((await viewport(page)).width - evidence.baseline.width)).toBeLessThanOrEqual(1);
            evidence.restored = true;
        } catch (error) {
            evidence.restoreError = error.stack || String(error);
            evidence.status = 'failed';
        }
        await writeFile(join(manifest.directory, 'native-zoom.json'), `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
        if (!evidence.restored) throw new Error('Native 400% qualification could not restore engine zoom; see native-zoom.json.');
    }
}
