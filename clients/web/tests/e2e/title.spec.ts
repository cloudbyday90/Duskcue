import { test, expect } from './fixtures';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const origin = '/media?type=series&sort=title&order=asc&watch=unwatched';
const seriesUrl = `/media/${mediaIds.series}?from=${encodeURIComponent(origin)}`;

test.describe('complete Tonight Title gallery', () => {
    test.use({ scenarioOptions: { browsePageSize: 1 } });

    test('loads all seasons and episodes and restores exact URL selection on reload', async ({ page, api }) => {
        await page.goto(seriesUrl);
        await expect(page.getByRole('heading', { name: 'Harbor Stories', exact: true })).toBeVisible();
        await expect(page.getByLabel('Season', { exact: true }).locator('option')).toHaveCount(2);
        await expect(page.getByRole('heading', { name: 'A New Arrival', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'The Last Ferry', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Episode 1 · A New Arrival', exact: true })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
        expect(api.requests.filter((request) => request.path === `/media-items/${mediaIds.seasonOne}/episodes`)).toHaveLength(2);

        await page.getByRole('link', { name: /Episode 2.*The Last Ferry/ }).focus();
        await page.keyboard.press('Enter');
        await expect(page.getByRole('heading', { name: 'Episode 2 · The Last Ferry', exact: true })).toBeVisible();
        expect(new URL(page.url()).searchParams.get('episode')).toBe(mediaIds.episodeTwo);
        await page.reload();
        await expect(page.getByRole('heading', { name: 'Episode 2 · The Last Ferry', exact: true })).toBeVisible();
        expect(new URL(page.url()).searchParams.get('from')).toBe(origin);
        await expect(page.getByRole('link', { name: /Back to browsing$/ })).toHaveAttribute('href', origin);
    });

    test('native season keyboard input retains focus and changes the complete gallery', async ({ page, api }) => {
        await page.goto(seriesUrl);
        const select = page.getByLabel('Season', { exact: true });
        await expect(select).toHaveValue(mediaIds.seasonOne);
        await select.focus();
        await page.keyboard.press('ArrowDown');
        await page.keyboard.press('Enter');
        await expect(select).toHaveValue(mediaIds.seasonTwo);
        await expect(select).toBeFocused();
        await expect(page.getByRole('heading', { name: 'Open Water', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Home Again', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'A New Arrival', exact: true })).toHaveCount(0);
        expect(new URL(page.url()).searchParams.get('season')).toBe(mediaIds.seasonTwo);
        expect(new URL(page.url()).searchParams.get('from')).toBe(origin);
    });

    test('selected episode watched, favorite, and rating controls mutate the exact identity', async ({ page, api }) => {
        await page.goto(`${seriesUrl}&season=${mediaIds.seasonOne}&episode=${mediaIds.episodeTwo}`);
        const actions = page.getByRole('group', { name: 'The Last Ferry', exact: true });
        await expect(actions.getByRole('button', { name: 'Mark watched', exact: true })).toBeEnabled();
        await actions.getByRole('button', { name: 'Mark watched', exact: true }).click();
        await expect(actions.getByRole('button', { name: 'Mark not watched', exact: true })).toBeVisible();
        await actions.getByRole('button', { name: 'Favorite', exact: true }).click();
        await expect(actions.getByRole('button', { name: 'Favorite', exact: true })).toHaveAttribute('aria-pressed', 'true');
        await actions.getByLabel('Your rating', { exact: true }).selectOption('6');
        await expect(actions.getByText('Saved', { exact: true })).toBeVisible();
        expect(api.watchByProfile[profileIds.alex][mediaIds.episodeTwo]).toMatchObject({ is_watched: true, is_favorite: true, user_rating: 6 });
        expect(api.watchByProfile[profileIds.alex][mediaIds.series].is_watched).toBe(false);
        expect(new URL(page.url()).searchParams.get('episode')).toBe(mediaIds.episodeTwo);
    });

    test('unavailable episodes retain selectable context and native file disclosure', async ({ page, api }) => {
        api.files[mediaIds.episodeTwo].forEach((file) => { file.is_healthy = false; });
        await page.goto(`${seriesUrl}&season=${mediaIds.seasonOne}&episode=${mediaIds.episodeTwo}`);
        await expect(page.getByRole('heading', { name: 'Episode 2 · The Last Ferry', exact: true })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeDisabled();
        const summary = page.getByText('Technical information and files', { exact: true }).first();
        await summary.focus();
        await page.keyboard.press('Enter');
        await expect(page.getByRole('button', { name: /^Play file / }).first()).toBeDisabled();
        await expect(summary).toBeFocused();
        await expect(page.getByRole('link', { name: /The Last Ferry/ })).toHaveAttribute('aria-current', 'true');
    });

    test('episode failure keeps title and origin and retries complete traversal', async ({ page, api }) => {
        api.failures.set(`GET /media-items/${mediaIds.seasonOne}/episodes`, { status: 503, detail: 'Fixture episode list unavailable' });
        await page.goto(seriesUrl);
        await expect(page.getByRole('heading', { name: 'Harbor Stories', exact: true })).toBeVisible();
        await expect(page.getByRole('alert')).toContainText('Fixture episode list unavailable');
        await expect(page.getByRole('link', { name: /Back to browsing$/ })).toHaveAttribute('href', origin);
        api.failures.delete(`GET /media-items/${mediaIds.seasonOne}/episodes`);
        await page.getByRole('button', { name: 'Try again', exact: true }).focus();
        await page.keyboard.press('Enter');
        await expect(page.getByRole('heading', { name: 'The Last Ferry', exact: true })).toBeVisible();
        await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
        await expect(page.getByRole('heading', { name: 'Episodes', exact: true })).toBeFocused();
    });

    test('unknown selected episodes are explained without substituting another playback target', async ({ page, api }) => {
        await page.goto(`${seriesUrl}&season=${mediaIds.seasonOne}&episode=00000000-0000-7000-8000-000000099999`);
        await expect(page.getByRole('alert')).toContainText('The selected season or episode is no longer available');
        await expect(page.getByRole('button', { name: /^(Play|Resume)$/ })).toHaveCount(0);
        await page.getByRole('link', { name: /Episode 2.*The Last Ferry/ }).click();
        await expect(page.getByRole('heading', { name: 'Episode 2 · The Last Ferry', exact: true })).toBeVisible();
    });

    test('direct episode metadata resolves to its actual series and season while retaining origin', async ({ page, api }) => {
        await page.goto(`/media/${mediaIds.episodeFour}?from=${encodeURIComponent(origin)}`);
        await expect(page.getByRole('heading', { name: 'Harbor Stories', exact: true })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Episode 2 · Home Again', exact: true })).toBeVisible();
        expect(new URL(page.url()).pathname).toBe(`/media/${mediaIds.series}`);
        expect(new URL(page.url()).searchParams.get('season')).toBe(mediaIds.seasonTwo);
        expect(new URL(page.url()).searchParams.get('episode')).toBe(mediaIds.episodeFour);
        expect(new URL(page.url()).searchParams.get('from')).toBe(origin);
    });
});

test('watch read failure keeps real title information and recovers with Retry', async ({ page, api }) => {
    api.failures.set(`GET /items/${mediaIds.resumeMovie}/watch-data`, { status: 503, detail: 'Fixture watch unavailable' });
    await page.goto(`/media/${mediaIds.resumeMovie}`);
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeDisabled();
    await expect(page.getByText('Watch status is unavailable. Refresh before starting playback or changing it.', { exact: true })).toBeVisible();
    api.failures.delete(`GET /items/${mediaIds.resumeMovie}/watch-data`);
    await page.getByRole('button', { name: 'Try again', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeFocused();
});

test('file read failure is visible beside playback and refreshes details without reloading metadata', async ({ page, api }) => {
    api.failures.set(`GET /media-items/${mediaIds.resumeMovie}/files`, { status: 503, detail: 'Fixture files unavailable' });
    await page.goto(`/media/${mediaIds.resumeMovie}`);
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeDisabled();
    await expect(page.getByRole('alert')).toHaveText('File information could not be loaded.');
    const metadataReads = api.requests.filter((request) => request.path === `/media-items/${mediaIds.resumeMovie}`).length;
    api.failures.delete(`GET /media-items/${mediaIds.resumeMovie}/files`);
    await page.getByRole('button', { name: 'Try again', exact: true }).first().click();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeFocused();
    expect(api.requests.filter((request) => request.path === `/media-items/${mediaIds.resumeMovie}`)).toHaveLength(metadataReads);
});

test('completed media offers Play despite a stale resume position and restores Resume after marking unwatched', async ({ page, api }) => {
    api.watchByProfile[profileIds.alex][mediaIds.resumeMovie].is_watched = true;
    await page.goto(`/media/${mediaIds.resumeMovie}`);
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeEnabled();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toHaveCount(0);
    await expect(page.getByRole('progressbar')).toHaveCount(0);
    await page.getByRole('button', { name: 'Mark not watched', exact: true }).click();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    await expect(page.getByRole('progressbar')).toBeVisible();
});

test('profile switching reloads isolated episode watch and favorite state while retaining Title selection', async ({ page, api }) => {
    api.watchByProfile[profileIds.morgan][mediaIds.episodeOne].is_watched = true;
    api.watchByProfile[profileIds.morgan][mediaIds.episodeOne].is_favorite = true;
    await page.goto(`${seriesUrl}&season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}`);
    const actions = page.getByRole('group', { name: 'A New Arrival', exact: true });
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    await expect(actions.getByRole('button', { name: 'Favorite', exact: true })).toHaveAttribute('aria-pressed', 'false');
    await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
    await page.getByRole('button', { name: /Morgan$/ }).click();
    await expect(actions.getByRole('button', { name: 'Mark not watched', exact: true })).toBeVisible();
    await expect(actions.getByRole('button', { name: 'Favorite', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeEnabled();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toHaveCount(0);
    expect(api.activeProfileId).toBe(profileIds.morgan);
    expect(new URL(page.url()).searchParams.get('episode')).toBe(mediaIds.episodeOne);
    expect(new URL(page.url()).searchParams.get('from')).toBe(origin);
    expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne]).toMatchObject({ is_watched: false, is_favorite: false, resume_position_ms: 300_000 });
});

test.describe('Title reflow', () => {
    test.use({ viewport: { width: 320, height: 844 } });
    test('artwork fallback and RTL gallery keep semantic controls without horizontal overflow', async ({ page, api }) => {
        await page.goto(seriesUrl);
        await expect(page.getByRole('heading', { name: 'The Last Ferry', exact: true })).toBeVisible();
        await page.evaluate(() => { document.documentElement.dir = 'rtl'; });
        await expect(page.getByLabel('Season', { exact: true })).toBeVisible();
        expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
        await page.screenshot({ path: '../../.cache/tonight-web/title-rtl-320.png', fullPage: true });
    });
});
