import { test, expect } from './playback-desktop-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { mediaIds } from '../fixtures/catalog.js';

const savedServer = { origin: 'http://server.test:48027', network_mode: 'local' as const, display_name: null, last_connected_at: null };
const titleUrl = `/media/${mediaIds.resumeMovie}?from=${encodeURIComponent('/media?type=movie')}`;

test.use({ desktopOptions: { server: savedServer, token: 'fixture-desktop-token' }, playbackOptions: { mode: 'transcode' } });

test('selected-server HLS requests carry bearer authentication through real decoding and player close', async ({ page, api, playback, desktop }) => {
    const requests: Array<{ url: string; authorization: string | undefined }> = [];
    page.on('request', (request) => {
        if (/\/api\/v1\/transcode\//.test(request.url())) requests.push({ url: request.url(), authorization: request.headers().authorization });
    });
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    await expectDecodedPlayback(page);
    expect(requests.length).toBeGreaterThanOrEqual(3);
    for (const request of requests) {
        expect(new URL(request.url).origin).toBe(savedServer.origin);
        expect(request.authorization).toBe('Bearer fixture-desktop-token');
    }
    expect(playback.starts[0].force_transcode).toBe(true);
    expect(api.requests.some((request) => request.path.startsWith('/stream/'))).toBe(false);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
    await expect.poll(() => playback.stops.length).toBe(1);
    expect(desktop.calls.filter((call) => /close|destroy/.test(call.command))).toHaveLength(0);
});

test('unsupported browser fullscreen uses mocked native window state and Escape leaves the stream open', async ({ page, playback, desktop }) => {
    await page.addInitScript(() => { Object.defineProperty(HTMLElement.prototype, 'requestFullscreen', { configurable: true, value: undefined }); });
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    await expectDecodedPlayback(page);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => desktop.fullscreen).toBe(true);
    await expect(region.getByRole('button', { name: 'Exit fullscreen', exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.fullscreenElement)).toBeNull();
    await page.keyboard.press('Escape');
    await expect.poll(() => desktop.fullscreen).toBe(false);
    expect(new URL(page.url()).pathname).toBe(`/play/${mediaIds.resumeMovie}`);
    expect(playback.stops).toHaveLength(0);
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => desktop.fullscreen).toBe(true);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
    await expect.poll(() => desktop.fullscreen).toBe(false);
    expect(desktop.calls.filter((call) => call.command === 'plugin:window|set_fullscreen').map((call) => call.args.value)).toEqual([true, false, true, false]);
    expect(desktop.calls.filter((call) => /close|destroy/.test(call.command))).toHaveLength(0);
});

test('native fullscreen failure stays labeled and recoverable while playback continues', async ({ page, playback, desktop }) => {
    await page.addInitScript(() => { Object.defineProperty(HTMLElement.prototype, 'requestFullscreen', { configurable: true, value: undefined }); });
    const region = await openPlayback(page, titleUrl, mediaIds.resumeMovie);
    await expectDecodedPlayback(page);
    desktop.failures.set('plugin:window|set_fullscreen', 'Fixture fullscreen unavailable');
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect(region.getByText('Could not change fullscreen. Try the fullscreen control again.', { exact: true })).toBeVisible();
    expect(desktop.fullscreen).toBe(false);
    expect(playback.starts).toHaveLength(1);
    desktop.failures.delete('plugin:window|set_fullscreen');
    await region.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await expect.poll(() => desktop.fullscreen).toBe(true);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expectTitleContext(page, titleUrl);
    await expect.poll(() => desktop.fullscreen).toBe(false);
});
