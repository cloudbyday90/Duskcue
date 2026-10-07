import type { Page } from '@playwright/test';
import { expect } from './playback-fixtures';

export async function openPlayback(page: Page, titleUrl: string, itemId: string) {
    await page.goto(titleUrl);
    await page.evaluate(() => {
        document.addEventListener('ended', (event) => {
            if (event.isTrusted && event.target instanceof HTMLVideoElement) event.target.dataset.nativeEnded = 'true';
        }, true);
    });
    await page.getByRole('button', { name: /^(Play|Resume)$/ }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe(`/play/${itemId}`);
    return page.getByRole('region', { name: 'Media player', exact: true });
}

export async function expectDecodedPlayback(page: Page) {
    const video = page.locator('video');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => ({
        width: element.videoWidth,
        height: element.videoHeight,
        ready: element.readyState >= 2,
        advancing: element.currentTime > 0.2,
        duration: Number.isFinite(element.duration) && element.duration > 0,
        paused: element.paused,
    })), { timeout: 10_000 }).toEqual({ width: 320, height: 180, ready: true, advancing: true, duration: true, paused: false });
    return video;
}

export async function expectTitleContext(page: Page, titleUrl: string) {
    const expected = new URL(titleUrl, 'http://fixture.test');
    await expect.poll(() => new URL(page.url()).pathname).toBe(expected.pathname);
    expect([...new URL(page.url()).searchParams.entries()].sort()).toEqual([...expected.searchParams.entries()].sort());
    await expect(page.getByRole('button', { name: /^(Play|Resume)$/ })).toBeEnabled();
}
