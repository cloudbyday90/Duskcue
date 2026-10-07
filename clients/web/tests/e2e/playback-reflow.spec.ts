import type { Locator, Page } from '@playwright/test';
import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const titleUrl = (episode = mediaIds.episodeOne) => `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${episode}&from=${encodeURIComponent('/media?type=series')}`;

async function geometry(control: Locator) {
    return control.evaluate((element) => {
        const box = element.getBoundingClientRect();
        const viewport = { width: window.innerWidth, height: window.innerHeight };
        const points = [
            [box.left + box.width / 2, box.top + box.height / 2],
            [box.left + box.width / 2, box.top + 1],
            [box.left + box.width / 2, box.bottom - 1],
            [box.left + 1, box.top + box.height / 2],
            [box.right - 1, box.top + box.height / 2],
        ];
        const hits = points.map(([x, y]) => {
            const hit = document.elementFromPoint(x, y);
            return { x, y, own: !!hit && (hit === element || element.contains(hit)), target: hit?.tagName || null };
        });
        let unfaded = true;
        const ancestors = [];
        for (let owner = element.parentElement; owner; owner = owner.parentElement) {
            const style = getComputedStyle(owner);
            if (Number(style.opacity) < 0.99 || style.visibility === 'hidden' || owner.hasAttribute('inert')) unfaded = false;
            const bounds = owner.getBoundingClientRect();
            ancestors.push({ tag: owner.tagName, className: owner.className, y: bounds.y, height: bounds.height, clientHeight: owner.clientHeight, scrollHeight: owner.scrollHeight, scrollTop: owner.scrollTop, overflowY: style.overflowY, position: style.position, transform: style.transform, scrollPaddingStart: style.scrollPaddingBlockStart, scrollPaddingEnd: style.scrollPaddingBlockEnd });
        }
        const style = getComputedStyle(element);
        return {
            box: { x: box.x, y: box.y, width: box.width, height: box.height }, viewport, hits,
            focused: document.activeElement === element,
            withinViewport: box.left >= -0.5 && box.top >= -0.5 && box.right <= viewport.width + 0.5 && box.bottom <= viewport.height + 0.5,
            usableTarget: box.width >= 43.5 && box.height >= 43.5,
            unobscured: hits.every((hit) => hit.own), unfaded, ancestors,
            scrollMarginStart: style.scrollMarginBlockStart, scrollMarginEnd: style.scrollMarginBlockEnd,
        };
    });
}

async function expectFocusedControl(control: Locator, label: string) {
    await expect(control).toBeFocused();
    try {
        await expect.poll(async () => {
            const value = await geometry(control);
            return { focused: value.focused, withinViewport: value.withinViewport, usableTarget: value.usableTarget, unobscured: value.unobscured, unfaded: value.unfaded };
        }).toEqual({ focused: true, withinViewport: true, usableTarget: true, unobscured: true, unfaded: true });
    } catch (error) {
        const snapshot = await geometry(control);
        console.log(JSON.stringify({ label, geometry: snapshot }));
        await test.info().attach(`${label}-geometry`, { body: Buffer.from(JSON.stringify(snapshot, null, 2)), contentType: 'application/json' });
        throw error;
    }
}

async function tabTo(page: Page, target: Locator, limit = 24) {
    for (let index = 0; index < limit; index += 1) {
        if (await target.evaluate((element) => document.activeElement === element)) return;
        await page.keyboard.press('Tab');
    }
    await expect(target).toBeFocused();
}

async function noHorizontalOverflow(page: Page, region: Locator) {
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
    expect(await region.evaluate((element) => element.scrollWidth <= element.clientWidth)).toBe(true);
}

test.use({ viewport: { width: 320, height: 180 }, scenarioOptions: { browsePageSize: 1 } });

test('short-height native controls remain reachable and unobscured through ordinary keyboard scrolling', async ({ page, playback }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Pause', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
    await region.focus();
    const required = new Set(['Close player', 'Seek', 'Play', 'Mute', 'Volume', 'Episodes', 'Settings', 'Fullscreen']);
    const seen = new Set<string>();
    for (let index = 0; index < 24 && seen.size < required.size; index += 1) {
        await page.keyboard.press('Tab');
        const focused = page.locator(':focus');
        const name = await focused.evaluate((element) => element.getAttribute('aria-label') || element.textContent?.trim() || '');
        expect(await focused.evaluate((element) => !!element.closest('[role="region"][aria-label="Media player"]'))).toBe(true);
        if (!required.has(name)) continue;
        await expectFocusedControl(focused, name);
        seen.add(name);
        if (name === 'Seek') {
            const position = Number(await focused.inputValue());
            await page.keyboard.press('ArrowRight');
            expect(Number(await focused.inputValue())).toBe(position + 100);
        }
    }
    expect([...seen].sort()).toEqual([...required].sort());
    await noHorizontalOverflow(page, region);
    await page.screenshot({ path: test.info().outputPath('player-reflow-native-controls-320x180.png') });
    const close = region.getByRole('button', { name: 'Close player', exact: true });
    await close.focus();
    await expectFocusedControl(close, 'Close-player');
    await page.keyboard.press('Enter');
    await expectTitleContext(page, titleUrl());
    expect(playback.starts).toHaveLength(1);
    expect(playback.stops).toHaveLength(1);
});

test('short-height disclosures scroll every choice into view and restore their trigger after selection', async ({ page, playback }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    const episodes = region.getByRole('button', { name: 'Episodes', exact: true });
    await episodes.focus();
    await expectFocusedControl(episodes, 'Episodes-trigger');
    await page.keyboard.press('Enter');
    const episodeMenu = region.getByRole('region', { name: 'Episodes', exact: true });
    const next = episodeMenu.getByRole('button', { name: 'Episode 2 · The Last Ferry', exact: true });
    await tabTo(page, next);
    await expectFocusedControl(next, 'Episode-two');
    await page.screenshot({ path: test.info().outputPath('player-reflow-episodes-320x180.png') });
    await page.keyboard.press('Enter');
    await expect.poll(() => playback.starts.length).toBe(2);
    await expectDecodedPlayback(page);
    await expectFocusedControl(episodes, 'Episodes-selection-return');
    const settings = region.getByRole('button', { name: 'Settings', exact: true });
    await settings.focus();
    await expectFocusedControl(settings, 'Settings-trigger');
    await page.keyboard.press('Enter');
    const menu = region.getByRole('region', { name: 'Settings', exact: true });
    const last = menu.getByRole('button', { name: '2×', exact: true });
    let maximumScroll = 0;
    for (let index = 0; index < 24 && !await last.evaluate((element) => document.activeElement === element); index += 1) {
        await page.keyboard.press('Tab');
        const focused = page.locator(':focus');
        expect(await focused.evaluate((element) => !!element.closest('[role="region"], section[aria-label="Settings"]')?.closest('.player-menus'))).toBe(true);
        await expectFocusedControl(focused, `Settings-choice-${index}`);
        maximumScroll = Math.max(maximumScroll, await menu.evaluate((element) => element.scrollTop));
    }
    await expectFocusedControl(last, 'Settings-last-choice');
    expect(maximumScroll).toBeGreaterThan(0);
    await noHorizontalOverflow(page, region);
    await page.screenshot({ path: test.info().outputPath('player-reflow-settings-320x180.png') });
    await page.keyboard.press('Enter');
    await expect.poll(() => page.locator('video').evaluate((element: HTMLVideoElement) => element.playbackRate)).toBe(2);
    await expectFocusedControl(settings, 'Settings-selection-return');
    await page.keyboard.press('Enter');
    await page.keyboard.press('Escape');
    await expectFocusedControl(settings, 'Settings-Escape-return');
    const close = region.getByRole('button', { name: 'Close player', exact: true });
    await close.focus();
    await expectFocusedControl(close, 'Close-after-menu');
    await page.keyboard.press('Enter');
    await expectTitleContext(page, titleUrl(mediaIds.episodeTwo));
});

test.describe('short-height trusted episode completion', () => {
    test.use({ playbackOptions: { clip: 'ending', autoplay: true } });

    test('Cancel and untimed Play next remain reachable without clipping under the sticky header', async ({ page, playback }) => {
        await page.emulateMedia({ reducedMotion: 'reduce' });
        await page.clock.install();
        const region = await openPlayback(page, titleUrl(), mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
        const play = region.getByRole('button', { name: 'Play now', exact: true });
        await expect(play).toBeVisible();
        await region.focus();
        await tabTo(page, play);
        await expectFocusedControl(play, 'Autoplay-play-now');
        await page.keyboard.press('Tab');
        const cancel = region.getByRole('button', { name: 'Cancel', exact: true });
        await expectFocusedControl(cancel, 'Autoplay-Cancel');
        await page.keyboard.press('Enter');
        const next = region.getByRole('button', { name: 'Play next', exact: true });
        await expectFocusedControl(next, 'Autoplay-untimed-next');
        await noHorizontalOverflow(page, region);
        await page.screenshot({ path: test.info().outputPath('player-reflow-cancelled-autoplay-320x180.png') });
        await page.clock.fastForward(11000);
        expect(playback.starts).toHaveLength(1);
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.starts.length).toBe(2);
        await expectDecodedPlayback(page);
        const close = region.getByRole('button', { name: 'Close player', exact: true });
        await close.focus();
        await expectFocusedControl(close, 'Autoplay-close');
        await page.keyboard.press('Enter');
        await expectTitleContext(page, titleUrl(mediaIds.episodeTwo));
    });
});
