// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { test, expect } from '@playwright/test';
import { exerciseVodDecoder } from '../native/vod-decoder-recovery.mjs';

const evidence = new WeakMap();
test.beforeEach(async ({ page }) => {
    const records = [];
    evidence.set(page, records);
    page.on('response', async (response) => {
        const path = new URL(response.url()).pathname;
        if (!path.startsWith('/api/v1/playback-commands') || path.includes('/outputs/') || records.length >= 128) return;
        const value = await response.json().catch(() => null);
        const request = response.request().postDataJSON();
        records.push({ method: response.request().method(), path, status: response.status(), etag: response.headers().etag,
            intent: request && { operation_id: request.operation_id, expected_output: request.expected_output, action: request.action?.kind },
            body: value && Object.fromEntries(['protocol', 'request_id', 'status', 'revision', 'state', 'output_revision', 'pending_operation', 'worker_recovery', 'operation_id', 'code'].map((key) => [key, value[key]])) });
    });
});
test.afterEach(async ({ page }, info) => {
    if (info.status === info.expectedStatus) return;
    const state = await page.evaluate(async () => {
        const module = await import(new URL('/src/lib/stores/player.js', location.origin).href);
        let value;
        const stop = module.player.subscribe((state) => { value = { phase: state.commandPhase, error: state.error?.message, status: state.error?.status }; });
        stop(); return value;
    }).catch(() => null);
    const path = info.outputPath('bounded-command-evidence.json');
    await writeFile(path, JSON.stringify({ records: evidence.get(page), state }));
    await info.attach('bounded-command-evidence', { path, contentType: 'application/json' });
});

async function fixture() {
    const path = process.env.DUSKCUE_VOD_CLIENT_FIXTURE;
    if (!path) throw new Error('Owned real backend fixture required');
    return JSON.parse(await readFile(path, 'utf8'));
}
async function seed(page, context, f) {
    await context.addCookies([{ name: 'session', value: f.token, domain: 'localhost', path: '/', httpOnly: true, sameSite: 'Strict' }]);
    await page.addInitScript(({ user, profile }) => {
        localStorage.setItem('duskcue_user', JSON.stringify({ id: user, username: `admission-${user}`, display_name: 'Qualification', role: 'member',
            capabilities: ['play_media'], has_all_library_access: true, active_profile_id: profile, profile_selection_required: false, locale: 'en' }));
        localStorage.setItem('duskcue_device_id', 'vod-command-browser-qualification');
    }, f);
}
async function open(page, context) {
    const f = await fixture();
    await seed(page, context, f);
    await page.goto(`/play/${f.item}?file=${f.file}`);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    await expect(region).toBeVisible();
    const video = region.locator('video');
    await expect(video).toBeVisible();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState >= 2 && element.videoWidth > 0)).toBe(true);
    if (await video.evaluate((element: HTMLVideoElement) => element.paused)) await region.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => (element.getVideoPlaybackQuality?.().totalVideoFrames || 0) > 0 && element.currentTime > .1)).toBe(true);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    return { region, video, f };
}
test('actual cookie Player retains viewer/history across direct seek, audio and quality changes, then confirms cleanup', async ({ page, context }) => {
    const commands = [];
    const paths = [];
    page.on('request', (request) => {
        const path = new URL(request.url()).pathname;
        paths.push(path);
        if (path === '/api/v1/playback-commands' && request.method() === 'POST') commands.push(request.postDataJSON());
    });
    const { region, video } = await open(page, context);
    expect(commands).toHaveLength(1);
    expect(commands[0].protocol).toBe('duskcue-vod-command/2');
    const first = await video.evaluate((element: HTMLVideoElement) => element.currentSrc);
    expect(first).toContain('/outputs/');
    expect(first).toMatch(/\/file$/);
    const seek = region.getByRole('slider', { name: 'Seek', exact: true });
    await seek.focus();
    await page.keyboard.press('ArrowRight'); await page.keyboard.press('ArrowRight');
    await page.keyboard.press('Tab');
    await expect.poll(() => video.evaluate((element: HTMLVideoElement, previous: string) => element.currentSrc !== previous && element.readyState >= 2, first)).toBe(true);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
    const beforeAudio = await video.evaluate((element: HTMLVideoElement) => element.currentSrc);
    const tracks = region.getByRole('button', { name: 'Audio & subtitles', exact: true });
    await tracks.focus(); await page.keyboard.press('Enter');
    const choices = region.getByRole('region', { name: 'Audio & subtitles', exact: true });
    const french = choices.getByRole('button', { name: 'French', exact: true });
    const alternate = await french.getAttribute('aria-pressed') === 'true' ? choices.getByRole('button', { name: 'English', exact: true }) : french;
    await expect(alternate).toHaveAttribute('aria-pressed', 'false');
    await alternate.click();
    await expect(tracks).toBeFocused();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement, previous: string) => element.currentSrc !== previous && element.readyState >= 2, beforeAudio)).toBe(true);
    const settings = region.getByRole('button', { name: 'Settings', exact: true });
    const beforeQuality = await video.evaluate((element: HTMLVideoElement) => element.currentSrc);
    await settings.click();
    await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Up to 1.5 Mbps', exact: true }).click();
    await expect(settings).toBeFocused();
    await expect.poll(() => commands.length).toBe(1);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement, previous: string) => element.currentSrc !== previous && element.readyState >= 2, beforeQuality)).toBe(true);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.paused)).toBe(true);
    await page.screenshot({ path: test.info().outputPath('real-vod-command-controls.png') });
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
    expect(paths.some((path) => path.endsWith('/withdraw'))).toBe(true);
    expect(paths.some((path) => path.endsWith('/settlement'))).toBe(true);
    expect(paths.some((path) => /^\/api\/v1\/(?:playback\/(?:start|seek|heartbeat|stop)|stream|transcode)(?:\/|$)/.test(path))).toBe(false);
    await page.getByRole('button', { name: /^(Play|Resume)$/, exact: true }).first().click();
    await expect(region).toBeVisible();
    await expect.poll(() => commands.length).toBe(2);
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState >= 2)).toBe(true);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
});

test('lost real Start observation stays detached across reload and settles without another Start', async ({ page, context }) => {
    let starts = 0;
    await page.route('**/api/v1/playback-commands', async (route) => {
        if (route.request().method() !== 'POST') return route.fallback();
        starts++;
        await route.fetch();
        await route.abort('connectionfailed');
    });
    const f = await fixture();
    await seed(page, context, f);
    await page.goto(`/play/${f.item}?file=${f.file}`);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    await expect(region.getByRole('button', { name: 'Check status', exact: true })).toBeVisible();
    expect(starts).toBe(1);
    await page.reload();
    await expect(region.getByRole('button', { name: 'Check status', exact: true })).toBeVisible();
    expect(starts).toBe(1);
    expect(await region.locator('video').getAttribute('src')).toBeNull();
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
    expect(starts).toBe(1);
});

test('lost real change acknowledgement reconciles the accepted output without another mutation', async ({ page, context }) => {
    let changes = 0;
    await page.route('**/api/v1/playback-commands/*/configuration', async (route) => {
        if (route.request().method() !== 'POST') return route.fallback();
        changes++;
        await route.fetch();
        await route.abort('connectionfailed');
    });
    const { region, video } = await open(page, context);
    const previous = await video.evaluate((element: HTMLVideoElement) => element.currentSrc);
    await region.getByRole('button', { name: 'Settings', exact: true }).click();
    await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Up to 1.5 Mbps', exact: true }).click();
    const check = region.getByRole('button', { name: 'Check status', exact: true });
    await expect(check).toBeVisible();
    await check.focus(); await page.keyboard.press('Enter');
    await expect(check).not.toBeVisible();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement, source: string) => element.currentSrc !== source && element.readyState >= 2, previous)).toBe(true);
    expect(changes).toBe(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
});

test('lost real renewal acknowledgement detaches until explicit status and does not replay renewal', async ({ page, context }) => {
    let renewals = 0;
    await page.route('**/api/v1/playback-commands/*/renew', async (route) => {
        renewals++;
        await route.fetch();
        await route.abort('connectionfailed');
    });
    const { region, video } = await open(page, context);
    const check = region.getByRole('button', { name: 'Check status', exact: true });
    await expect(check).toBeVisible({ timeout: 25000 });
    expect(renewals).toBe(1);
    await expect(video).not.toHaveAttribute('src');
    await check.click();
    await expect(check).not.toBeVisible();
    await expect.poll(() => video.evaluate((element: HTMLVideoElement) => element.readyState >= 2)).toBe(true);
    expect(renewals).toBe(1);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
});

test('lost withdrawal acknowledgement retains cleanup until an explicit same-identity retry', async ({ page, context }) => {
    const withdrawn = [];
    await page.route('**/api/v1/playback-commands/*/withdraw', async (route) => {
        withdrawn.push(new URL(route.request().url()).pathname);
        if (withdrawn.length !== 1) return route.fallback();
        await route.fetch();
        await route.abort('connectionfailed');
    });
    const { region } = await open(page, context);
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region.getByRole('button', { name: 'Retry closing playback', exact: true })).toBeVisible();
    expect(withdrawn).toHaveLength(1);
    await region.getByRole('button', { name: 'Retry closing playback', exact: true }).click();
    await expect(region).not.toBeVisible();
    expect(withdrawn).toHaveLength(2);
    expect(withdrawn[0]).toBe(withdrawn[1]);
});

test('genuine copied AVC decoder failure automatically recovers through the retained protocol', async ({ page, context }) => {
    const f = await fixture();
    await seed(page, context, f);
    const recoveries = [];
    page.on('response', (response) => {
        const request = response.request();
        if (request.method() !== 'POST' || !new URL(request.url()).pathname.endsWith('/configuration')) return;
        const body = request.postDataJSON();
        if (body?.action?.kind === 'recover') recoveries.push({ status: response.status(), evidenceKind: body.action.evidence?.kind });
    });
    await page.goto(`/play/${f.recovery.item}?file=${f.recovery.file}`);
    const region = page.getByRole('region', { name: 'Media player', exact: true });
    await expect(region).toBeVisible();
    await exerciseVodDecoder({ page, region, video: region.locator('video'), recoveries: () => recoveries, selectAudio: true });
    await region.getByRole('button', { name: 'Close player', exact: true }).click();
    await expect(region).not.toBeVisible();
});
