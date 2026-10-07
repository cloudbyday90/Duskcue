import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, openPlayback } from './playback-journey';
import { installApiFixture } from '../fixtures/api.js';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent('/media?type=series')}`;

test.use({ playbackOptions: { clip: 'ending', autoplay: true }, scenarioOptions: { browsePageSize: 1 } });

test('a real profile switch in another tab stops the completed session once and disposes its countdown', async ({ page, api, playback }) => {
    await page.clock.install();
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
    const other = await page.context().newPage();
    const errors: string[] = [];
    other.on('pageerror', (error) => errors.push(error.message));
    try {
        await installApiFixture(other, api);
        await other.goto('/dashboard');
        await other.getByRole('button', { name: 'User menu', exact: true }).click();
        await other.getByRole('button', { name: /Morgan$/ }).click();
        await expect.poll(() => api.activeProfileId).toBe(profileIds.morgan);
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        await expect(page.getByRole('button', { name: 'User menu', exact: true })).toHaveText(/Morgan/);
        await expect(page.locator('video')).toHaveCount(0);
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        await expect.poll(() => playback.stops.length).toBe(1);
        expect(playback.stops[0].session_id).toBe(playback.starts[0].session_id);
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].is_watched).toBe(true);
        expect(api.watchByProfile[profileIds.morgan][mediaIds.episodeOne].is_watched).toBe(false);
        expect(errors).toEqual([]);
    } finally { await other.close(); }
});

for (const status of [401, 503]) {
test(`failed playback release (${status}) does not trap explicit sign-out or replay the retained old release`, async ({ page, api, playback }) => {
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    api.failures.set('POST /playback/stop', { status, detail: 'Fixture playback release unavailable' });
    const other = await page.context().newPage();
    try {
        await installApiFixture(other, api);
        await other.goto('/dashboard');
        await other.getByRole('button', { name: 'User menu', exact: true }).click();
        await other.getByRole('button', { name: /Morgan$/ }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        await expect(region).toHaveCount(0);
        await expect(page.getByRole('button', { name: 'Retry closing stream', exact: true })).toBeVisible();
        await page.getByRole('button', { name: 'User menu', exact: true }).click();
        await page.getByRole('button', { name: 'Sign out', exact: true }).click();
        await expect(page).toHaveURL(/\/auth\/login$/);
        await expect(page.getByRole('button', { name: 'Retry closing stream', exact: true })).toHaveCount(0);
        expect(playback.stopAttempts).toHaveLength(1);
        expect(playback.stops).toHaveLength(0);
        expect(api.requests.filter((request) => request.method === 'POST' && request.path === '/auth/logout')).toHaveLength(1);
    } finally { await other.close(); }
});
}

test('a failed stop during a cross-tab profile switch clears old playback and retains an explicit same-account cleanup retry', async ({ page, api, playback }) => {
    await page.clock.install();
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    const video = await expectDecodedPlayback(page);
    await expect(video).toHaveAttribute('data-native-ended', 'true', { timeout: 7000 });
    await expect(region.getByRole('button', { name: 'Play now', exact: true })).toBeVisible();
    api.failures.set('POST /playback/stop', { status: 503, detail: 'Fixture stop temporarily unavailable' });
    const other = await page.context().newPage();
    try {
        await installApiFixture(other, api);
        await other.goto('/dashboard');
        await other.getByRole('button', { name: 'User menu', exact: true }).click();
        await other.getByRole('button', { name: /Morgan$/ }).click();
        await expect.poll(() => api.activeProfileId).toBe(profileIds.morgan);
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        await expect(page.getByRole('button', { name: 'User menu', exact: true })).toHaveText(/Morgan/);
        await expect(page.locator('video')).toHaveCount(0);
        const retry = page.getByRole('button', { name: 'Retry closing stream', exact: true });
        await expect(retry).toBeVisible();
        await expect(page.getByRole('status').filter({ hasText: 'The previous stream could not be closed.' })).toBeVisible();
        await page.clock.fastForward(12_000);
        expect(playback.starts).toHaveLength(1);
        expect(playback.stops).toHaveLength(0);
        const failedAttempts = api.requests.filter((request) => request.method === 'POST' && request.path === '/playback/stop');
        expect(failedAttempts).toHaveLength(1);
        expect(failedAttempts[0].body.session_id).toBe(playback.starts[0].session_id);
        await retry.focus();
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.failureResponses.filter((request) => request.method === 'POST' && request.path === '/playback/stop').length).toBe(2);
        await expect(retry).toBeFocused();
        expect(api.requests.filter((request) => request.method === 'POST' && request.path === '/playback/stop')[1].body).toEqual(failedAttempts[0].body);
        api.failures.delete('POST /playback/stop');
        await page.keyboard.press('Enter');
        await expect.poll(() => playback.stops.length).toBe(1);
        expect(playback.stops[0]).toEqual(failedAttempts[0].body);
        await expect(retry).toHaveCount(0);
        await expect(page.locator('#main-content')).toBeFocused();
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].is_watched).toBe(true);
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(1);
        expect(api.watchByProfile[profileIds.morgan][mediaIds.episodeOne].is_watched).toBe(false);
        expect(playback.starts).toHaveLength(1);
    } finally { await other.close(); }
});
