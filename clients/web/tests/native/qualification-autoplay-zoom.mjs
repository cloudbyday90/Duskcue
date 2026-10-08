// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { mediaIds } from '../fixtures/catalog.js';
import { invokeNative } from './qualification-api.mjs';
import { keyboardReach } from './qualification-focus.mjs';
import { releaseNativePlaybackForCleanup } from './qualification-playback.mjs';
import { captureNativeArtifact, createNativeCaptureInventory } from './qualification-capture.mjs';
import { warmHostedWindowControl } from './qualification-window-control.mjs';
import { exerciseRealBackgroundCountdown } from './qualification-background-countdown.mjs';

async function saveAutoplay(page, appOrigin, enabled) {
    await page.goto(`${appOrigin}/settings/preferences`);
    await page.getByLabel('Play the next episode automatically', { exact: true }).setChecked(enabled);
    const save = page.getByRole('button', { name: 'Save changes', exact: true });
    if (await save.isEnabled()) {
        await save.click();
        await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
    }
}

export async function exerciseAutoplayZoom(page, manifest, api, inventory = createNativeCaptureInventory(), runtime) {
    const evidence = { status: 'in_progress', focus: {}, contrast: inventory.contrast, screenshots: [], restored: false };
    const appOrigin = new URL(page.url()).origin;
    const screenshot = async (name) => {
        const path = await captureNativeArtifact(page, manifest, inventory, name, false);
        evidence.screenshots.push(path);
    };
    const zoom = (value) => invokeNative(page, 'plugin:webview|set_webview_zoom', { label: 'main', value });
    let windowControl;
    try {
        await saveAutoplay(page, appOrigin, true);
        await zoom(1);
        const base = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio }));
        await zoom(4);
        await expect.poll(async () => Math.abs(await page.evaluate(() => innerWidth) - base.width / 4)).toBeLessThanOrEqual(1);
        evidence.viewport = await page.evaluate(() => ({ width: innerWidth, height: innerHeight, dpr: devicePixelRatio }));
        expect(evidence.viewport.width).toBeLessThanOrEqual(320);
        expect(evidence.viewport.dpr / base.dpr).toBeCloseTo(4, 2);
        expect(Math.abs(evidence.viewport.height - base.height / 4)).toBeLessThanOrEqual(1);
        if (manifest.hostedCI) windowControl = await warmHostedWindowControl(runtime, manifest);
        else evidence.background = { status: 'not_executed', reason: 'Real window actions are restricted to an owned hosted Windows job.' };

        await page.goto(`${appOrigin}/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}`);
        await page.getByRole('button', { name: /^(Play|Resume)$/ }).click();
        const region = page.getByRole('region', { name: 'Media player', exact: true });
        const video = page.locator('video');
        await expect.poll(() => video.evaluate((element) => element.videoWidth > 0 && element.currentTime > 0.2 && !element.paused), { timeout: 15_000 }).toBe(true);
        const playback = api.getPlayback();
        const starts = playback.starts.length;
        const stops = playback.stops.length;
        await page.evaluate(() => {
            document.addEventListener('ended', (event) => {
                if (event.isTrusted && event.target instanceof HTMLVideoElement) event.target.dataset.nativeCountdownEnded = 'true';
            }, { capture: true, once: true });
        });
        await video.evaluate((element) => { element.currentTime = element.duration - 0.5; });
        await expect(video).toHaveAttribute('data-native-countdown-ended', 'true', { timeout: 10_000 });
        const playNow = region.getByRole('button', { name: 'Play now', exact: true });
        await expect(playNow).toBeVisible();
        if (windowControl) evidence.background = await exerciseRealBackgroundCountdown({ page, api, control: windowControl, screenshot });
        await keyboardReach(page, playNow, 'countdown-play-now', evidence);
        const countdown = region.locator('.autoplay-card .countdown');
        const held = await countdown.innerText();
        await new Promise((resolve) => setTimeout(resolve, 1_250));
        await expect(countdown).toHaveText(held);
        await keyboardReach(page, region.getByRole('button', { name: 'Cancel', exact: true }), 'countdown-cancel', evidence);
        await screenshot('native-400-countdown-actions');
        await page.keyboard.press('Enter');
        const playNext = region.getByRole('button', { name: 'Play next', exact: true });
        await expect(playNow).toHaveCount(0);
        await keyboardReach(page, playNext, 'cancelled-untimed-next', evidence);
        await new Promise((resolve) => setTimeout(resolve, 11_000));
        expect(playback.starts.length).toBe(starts);
        await expect(playNext).toBeVisible();
        await screenshot('native-400-countdown-cancelled');
        await keyboardReach(page, region.getByRole('button', { name: 'Close player', exact: true }), 'cancelled-close', evidence);
        await page.keyboard.press('Enter');
        await expect.poll(() => new URL(page.url()).pathname).toBe(`/media/${mediaIds.series}`);
        await expect.poll(() => playback.stops.length).toBe(stops + 1);
        evidence.status = 'passed';
        evidence.actualTrustedEnded = true;
        evidence.cardFocusPausedCountdown = true;
        evidence.cancelStayedUntimedPastOriginalDeadline = true;
        return evidence;
    } catch (error) {
        evidence.status = 'failed';
        evidence.error = error.stack || String(error);
        if (error.backgroundProof) evidence.background = error.backgroundProof;
        await screenshot('native-400-countdown-failure').catch(() => {});
        throw error;
    } finally {
        let windowCleanupFailure;
        try { await windowControl?.close(); }
        catch (error) { windowCleanupFailure = error; evidence.windowRestoreError = error.message; }
        try {
            await zoom(1);
            await releaseNativePlaybackForCleanup(page, api);
            await saveAutoplay(page, appOrigin, false);
            evidence.restored = true;
        } catch (error) { evidence.restoreError = error.message; evidence.status = 'failed'; }
        if (windowCleanupFailure) { evidence.restored = false; evidence.status = 'failed'; }
        await writeFile(join(manifest.directory, 'native-autoplay-zoom.json'), `${JSON.stringify(evidence, null, 2)}\n`, 'utf8');
        if (!evidence.restored) throw new Error('The native countdown zoom test could not restore zoom/preferences; see native-autoplay-zoom.json.');
    }
}
