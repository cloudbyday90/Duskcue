import type { Page, Route } from '@playwright/test';
import { test, expect } from './playback-fixtures';
import { expectDecodedPlayback, expectTitleContext, openPlayback } from './playback-journey';
import { createApiScenario, installApiFixture } from '../fixtures/api.js';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

const titleUrl = `/media/${mediaIds.series}?season=${mediaIds.seasonOne}&episode=${mediaIds.episodeOne}&from=${encodeURIComponent('/media?type=series')}`;

async function installAuthorizationApi(page: Page, api: ReturnType<typeof createApiScenario>) {
    const state = {
        sessionStatus: 200 as number | 'network',
        sessionProbe: null as null | ((route: Route) => Promise<void>),
        loginUser: { ...api.user },
    };
    await page.route(/\/api\/v1(?:\/|$)/, async (route) => {
        const request = route.request();
        const path = new URL(request.url()).pathname.replace('/api/v1', '');
        const method = request.method();
        if (!(method === 'GET' && ['/user/sessions', '/user/preferences'].includes(path)) && !(method === 'POST' && path === '/auth/login')) return route.fallback();
        api.requests.push({ method, path, body: request.postData() ? request.postDataJSON() : null, query: {} });
        const json = (body: object, status = 200) => route.fulfill({ status, contentType: status >= 400 ? 'application/problem+json' : 'application/json', body: JSON.stringify(body) });
        if (path === '/user/sessions') {
            if (state.sessionProbe) return state.sessionProbe(route);
            if (state.sessionStatus === 'network') return route.abort('connectionreset');
            return state.sessionStatus === 200 ? json({ items: [] }) : json({ status: state.sessionStatus, title: 'FIXTURE_SESSION', detail: 'Fixture session validation unavailable' }, state.sessionStatus);
        }
        if (path === '/user/preferences') return json({ locale: 'en', available_locales: [{ tag: 'en', name: 'English' }] });
        api.user = { ...state.loginUser };
        api.authenticated = true;
        api.selectionRequired = !!api.user.profile_selection_required;
        api.activeProfileId = api.user.active_profile_id;
        api.parentUnlockRequired = api.activeProfileId === profileIds.kids;
        return json({ user: api.user, session_token: 'fixture-login-session' });
    });
    return state;
}

async function signIn(page: Page) {
    await page.getByRole('button', { name: 'Password', exact: true }).click();
    await page.getByLabel('Username', { exact: true }).fill('fixture');
    await page.getByLabel('Password', { exact: true }).fill('fixture-password');
    await page.getByRole('button', { name: 'Sign In', exact: true }).click();
}

test('definitive current-session expiry releases the physical player and preserves its exact Title through sign-in and required profile selection', async ({ page, api, playback }) => {
    const authorization = await installAuthorizationApi(page, api);
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    authorization.sessionStatus = 401;
    authorization.loginUser = { ...api.user, active_profile_id: null, profile_selection_required: true };
    api.failures.set('POST /playback/stop', { status: 401, detail: 'Fixture expired playback credential' });
    await region.getByRole('button', { name: 'Close player', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
    expect(new URL(page.url()).searchParams.get('return_to')).toBe(titleUrl);
    await expect(page.getByRole('heading', { name: 'Sign In', exact: true })).toBeVisible();
    await expect(page.locator('video')).toHaveCount(0);
    await expect(page.getByRole('button', { name: /^Retry closing/ })).toHaveCount(0);
    expect(await page.evaluate(() => localStorage.getItem('duskcue_user'))).toBeNull();
    expect(api.requests.filter((request) => request.path === '/user/sessions')).toHaveLength(1);
    expect(api.requests.filter((request) => request.path === '/auth/logout')).toHaveLength(0);
    expect(playback.stopAttempts).toHaveLength(1);
    expect(playback.stops).toHaveLength(0);
    authorization.sessionStatus = 200;
    await signIn(page);
    await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
    expect(new URL(page.url()).pathname).toBe(`/media/${mediaIds.series}`);
    expect(api.requests.filter((request) => request.path === `/profiles/${profileIds.alex}/switch`)).toHaveLength(0);
    await page.getByRole('button', { name: 'Alex', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expectTitleContext(page, titleUrl);
    await expect(page.getByRole('button', { name: 'User menu', exact: true })).toHaveText(/Alex/);
    expect(api.requests.filter((request) => request.path === `/profiles/${profileIds.alex}/switch`)).toHaveLength(1);
    expect(playback.starts).toHaveLength(1);
    expect(playback.stopAttempts).toHaveLength(1);
    expect(api.watchByProfile[profileIds.alex][mediaIds.episodeOne].play_count).toBe(0);
});

for (const sessionStatus of [200, 503, 'network'] as const) {
    test(`a Stop 401 with ${sessionStatus} session validation preserves auth and focused explicit release recovery`, async ({ page, api, playback }) => {
        const authorization = await installAuthorizationApi(page, api);
        const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
        const video = await expectDecodedPlayback(page);
        authorization.sessionStatus = sessionStatus;
        api.failures.set('POST /playback/stop', { status: 401, detail: 'Fixture endpoint-specific release denial' });
        await region.getByRole('button', { name: 'Close player', exact: true }).focus();
        await page.keyboard.press('Enter');
        const retry = region.getByRole('button', { name: 'Retry closing playback', exact: true });
        await expect(retry).toBeFocused();
        await expect(region.getByRole('status').filter({ hasText: 'Could not finish closing playback.' })).toBeVisible();
        expect(new URL(page.url()).pathname).toBe(`/play/${mediaIds.episodeOne}`);
        expect(await video.evaluate((element: HTMLVideoElement) => ({ paused: element.paused, source: element.getAttribute('src') }))).toEqual({ paused: true, source: null });
        expect(await page.evaluate(() => JSON.parse(localStorage.getItem('duskcue_user')!).id)).toBe(api.user.id);
        expect(api.requests.filter((request) => request.path === '/user/sessions')).toHaveLength(1);
        expect(api.requests.filter((request) => request.path === '/auth/logout')).toHaveLength(0);
        await page.clock.install();
        await page.clock.fastForward(16_000);
        expect(playback.stopAttempts).toHaveLength(1);
        expect(playback.starts).toHaveLength(1);
        api.failures.delete('POST /playback/stop');
        await page.keyboard.press('Enter');
        await expectTitleContext(page, titleUrl);
        expect(playback.stopAttempts).toHaveLength(2);
        expect(playback.stopAttempts[1]).toEqual(playback.stopAttempts[0]);
        expect(playback.stops).toHaveLength(1);
    });
}

test('a superseded session probe is actually canceled during cross-tab profile transition and cannot clear a subsequently signed-in account', async ({ page, api, playback }) => {
    const authorization = await installAuthorizationApi(page, api);
    const region = await openPlayback(page, titleUrl, mediaIds.episodeOne);
    await expectDecodedPlayback(page);
    let finishProbe!: () => void;
    const pendingProbe = new Promise<void>((resolve) => { finishProbe = resolve; });
    let deliveredProbe = false;
    authorization.sessionProbe = async (route) => {
        await pendingProbe;
        await route.fulfill({ status: 401, contentType: 'application/problem+json', body: JSON.stringify({ status: 401, title: 'FIXTURE_SESSION', detail: 'Fixture obsolete credential expired' }) });
        deliveredProbe = true;
    };
    api.failures.set('POST /playback/stop', { status: 401, detail: 'Fixture old account release denied' });
    const other = await page.context().newPage();
    try {
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect.poll(() => api.requests.filter((request) => request.path === '/user/sessions').length).toBe(1);
        const failedProbe = page.waitForEvent('requestfailed', { predicate: (request) => new URL(request.url()).pathname === '/api/v1/user/sessions' });
        await installApiFixture(other, api);
        await other.goto('/dashboard');
        await other.getByRole('button', { name: 'User menu', exact: true }).click();
        await other.getByRole('button', { name: /Morgan$/ }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        expect((await failedProbe).failure()).toEqual({ errorText: 'net::ERR_ABORTED' });
        await expect(page.locator('video')).toHaveCount(0);
        await page.getByRole('button', { name: 'User menu', exact: true }).click();
        await page.getByRole('button', { name: 'Sign out', exact: true }).click();
        await expect(page).toHaveURL(/\/auth\/login$/);
        const newerId = '00000000-0000-7000-8000-000000009901';
        authorization.loginUser = { ...api.user, id: newerId, active_profile_id: profileIds.morgan, profile_selection_required: false };
        await signIn(page);
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        await expect(page.getByRole('button', { name: 'User menu', exact: true })).toHaveText(/Morgan/);
        finishProbe();
        await expect.poll(() => deliveredProbe).toBe(true);
        await expect.poll(() => page.evaluate(() => JSON.parse(localStorage.getItem('duskcue_user')!).id)).toBe(newerId);
        expect(new URL(page.url()).pathname).toBe('/dashboard');
        await expect(page.getByRole('button', { name: /^Retry closing/ })).toHaveCount(0);
        expect(playback.stopAttempts).toHaveLength(1);
        expect(playback.starts).toHaveLength(1);
        expect(api.requests.filter((request) => request.path === '/auth/logout')).toHaveLength(1);
    } finally { finishProbe(); await other.close(); }
});
