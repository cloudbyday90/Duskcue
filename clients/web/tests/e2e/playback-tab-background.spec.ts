/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import type { CDPSession, Page } from '@playwright/test';
import { test as playbackTest, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';
import { launchRealTabBrowser } from '../fixtures/real-tab-browser.mjs';

const test = playbackTest.extend<{ realTabBrowser: Awaited<ReturnType<typeof launchRealTabBrowser>> }>({
    realTabBrowser: [async ({ playwright }, use, testInfo) => {
        const owned = await launchRealTabBrowser(playwright.chromium);
        try { await use(owned); } finally {
            const cleanup = await Promise.allSettled([owned.dispose()]);
            await testInfo.attach('owned-real-tab-browser', { body: JSON.stringify(owned.proof), contentType: 'application/json' });
            if (cleanup[0].status === 'rejected') throw cleanup[0].reason;
        }
    }, { timeout: 75_000 }],
    context: async ({ realTabBrowser }, use) => { await use(realTabBrowser.context); },
});

const episodeUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent('/media?type=series')}`;
const pauseMs = 11_000;

type VisibilityObservation = { state: string; hidden: boolean; trusted: boolean; time: number };

async function observe(page: Page) {
    return page.evaluate(() => {
        const events: VisibilityObservation[] = JSON.parse(document.documentElement.dataset.tabVisibilityEvents || '[]');
        const video = document.querySelector('video');
        return {
            hidden: document.hidden,
            state: document.visibilityState,
            time: performance.now(),
            events,
            countdown: document.querySelector('.autoplay-card .countdown')?.textContent,
            cardFocused: document.querySelector('.autoplay-card')?.contains(document.activeElement) === true,
            menuOpen: document.querySelector('.player-popover:not([hidden])') !== null,
            videoEnded: video?.ended,
            source: video?.currentSrc,
        };
    });
}

test.use({ headless: false, playbackOptions: { mode: 'transcode', clip: 'ending', autoplay: true } });

test('real tab backgrounding pauses remaining countdown and Cancel stays untimed after restoration', async ({ page, api, playback, headless, baseURL, realTabBrowser }) => {
    test.setTimeout(60_000);
    expect(headless).toBe(false);
    const observations: Array<Record<string, unknown>> = [];
    const sessions: CDPSession[] = [];
    const primaryFailures: unknown[] = [];
    expect(realTabBrowser.proof.noDefaults).toBe(true);
    expect(realTabBrowser.proof.headlessArguments).toEqual([]);
    await page.setViewportSize({ width: 1280, height: 720 });
    const other = await page.context().newPage();
    try {
        await other.goto('about:blank');
        sessions.push(await page.context().newCDPSession(page));
        sessions.push(await page.context().newCDPSession(other));
        const windows = await Promise.all(sessions.map((session) => session.send('Browser.getWindowForTarget')));
        observations.push({ phase: 'browser-topology', defaultContextNoDefaults: true, headlessArguments: realTabBrowser.proof.headlessArguments, playerWindowId: windows[0].windowId, otherWindowId: windows[1].windowId });
        expect(windows[0].windowId).toBeGreaterThan(0);
        expect(windows[1].windowId).toBe(windows[0].windowId);
        await page.bringToFront();
        await page.addInitScript(() => {
            document.addEventListener('visibilitychange', (event) => {
                const events = JSON.parse(document.documentElement.dataset.tabVisibilityEvents || '[]');
                events.push({ state: document.visibilityState, hidden: document.hidden, trusted: event.isTrusted, time: performance.now() });
                document.documentElement.dataset.tabVisibilityEvents = JSON.stringify(events.slice(-8));
            });
        });
        const region = await openPlayback(page, new URL(episodeUrl, baseURL).href, mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 10_000 });
        await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.ended)).toBe(true);
        expect(api.requests.some((request) => request.path.endsWith('/manifest.m3u8'))).toBe(true);
        await region.focus();
        await expect.poll(() => observe(page).then((state) => state.countdown)).toMatch(/^Playing next in [3-8]s$/);
        const foreground = await observe(page);
        expect(foreground).toMatchObject({ hidden: false, state: 'visible', cardFocused: false, menuOpen: false });
        observations.push({ phase: 'before-background', ...foreground, starts: playback.starts.length });

        await other.bringToFront();
        observations.push({ phase: 'background-attempt', ...await observe(page), starts: playback.starts.length });
        await expect.poll(async () => {
            const state = await observe(page);
            return state.hidden && state.state === 'hidden' && state.events.some((event) => event.hidden && event.state === 'hidden' && event.trusted && event.time >= foreground.time);
        }, { timeout: 5000, message: 'The browser must produce a real trusted Hidden transition; unavailable backgrounding cannot qualify T07.' }).toBe(true);
        await expect.poll(() => observe(page).then((state) => state.countdown)).toMatch(/^Countdown paused at [1-8]s$/);
        const paused = await observe(page);
        const remaining = Number(/^Countdown paused at (\d+)s$/.exec(paused.countdown || '')?.[1]);
        expect(remaining).toBeGreaterThan(0);
        expect(remaining).toBeLessThan(10);
        expect(paused).toMatchObject({ cardFocused: false, menuOpen: false, videoEnded: true, source: foreground.source });
        const began = performance.now();
        await new Promise((resolve) => setTimeout(resolve, pauseMs));
        const backgroundElapsedMs = performance.now() - began;
        expect(backgroundElapsedMs).toBeGreaterThanOrEqual(pauseMs);
        const held = await observe(page);
        observations.push({ phase: 'background-held', ...held, starts: playback.starts.length, backgroundElapsedMs });
        expect(held).toMatchObject({ hidden: true, state: 'hidden', countdown: paused.countdown, cardFocused: false, menuOpen: false, source: foreground.source });
        expect(playback.starts).toHaveLength(1);
        expect(playback.stops).toHaveLength(0);

        const hiddenEvent = [...paused.events].reverse().find((event) => event.hidden && event.trusted && event.time >= foreground.time)!;
        await page.bringToFront();
        await expect.poll(async () => {
            const state = await observe(page);
            return !state.hidden && state.state === 'visible' && state.events.some((event) => !event.hidden && event.state === 'visible' && event.trusted && event.time >= hiddenEvent.time);
        }).toBe(true);
        await expect.poll(() => observe(page).then((state) => state.countdown)).toMatch(/^Playing next in \d+s$/);
        const restored = await observe(page);
        const restoredSeconds = Number(/^Playing next in (\d+)s$/.exec(restored.countdown || '')?.[1]);
        expect(restoredSeconds).toBeGreaterThan(0);
        expect(restoredSeconds).toBeLessThanOrEqual(remaining);
        expect(restoredSeconds).toBeGreaterThanOrEqual(remaining - 1);
        expect(restored).toMatchObject({ cardFocused: false, menuOpen: false });
        observations.push({ phase: 'restored', ...restored, starts: playback.starts.length });
        await expect.poll(async () => Number(/^Playing next in (\d+)s$/.exec((await observe(page)).countdown || '')?.[1]), { timeout: 3000 }).toBeLessThan(remaining);
        expect(playback.starts).toHaveLength(1);
        await region.getByRole('button', { name: 'Cancel', exact: true }).click();
        await expect(region.getByRole('button', { name: 'Play next', exact: true })).toBeVisible();
        await region.focus();
        const cancelBegan = performance.now();
        await new Promise((resolve) => setTimeout(resolve, pauseMs));
        expect(performance.now() - cancelBegan).toBeGreaterThanOrEqual(pauseMs);
        const cancelled = await observe(page);
        observations.push({ phase: 'cancelled-untimed', ...cancelled, starts: playback.starts.length });
        expect(cancelled).toMatchObject({ hidden: false, cardFocused: false, menuOpen: false, source: foreground.source });
        expect(cancelled.countdown).toBeUndefined();
        await expect(region.getByRole('button', { name: 'Play next', exact: true })).toBeVisible();
        expect(playback.starts).toHaveLength(1);
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expectTitleContext(page, episodeUrl);
        expect(playback.stops.map((stop) => stop.session_id)).toEqual([playback.starts[0].session_id]);
    } catch (error) {
        primaryFailures.push(error);
        throw error;
    } finally {
        try {
            if (!page.isClosed()) observations.push({ phase: 'final', ...await observe(page), starts: playback.starts.length, stops: playback.stops.length });
        } catch (error) { observations.push({ phase: 'final-observation-unavailable', error: String(error) }); }
        const detached = await Promise.allSettled(sessions.map((session) => session.detach()));
        const cleanupFailures = detached.flatMap((result) => result.status === 'rejected' ? [result.reason] : []);
        try { if (!other.isClosed()) await other.close(); } catch (error) { cleanupFailures.push(error); }
        if (cleanupFailures.length) observations.push({ phase: 'cleanup-failed', errors: cleanupFailures.map(String) });
        await test.info().attach('actual-tab-visibility', { body: JSON.stringify({ headless, browserVersion: page.context().browser()?.version(), observations }), contentType: 'application/json' });
        if (cleanupFailures.length) throw new AggregateError([...primaryFailures, ...cleanupFailures], 'Real-tab test cleanup failed');
    }
});
