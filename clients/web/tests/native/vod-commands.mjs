// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { readQualification } from './qualification-config.mjs';
import { launchQualification, stopQualification } from './qualification-runtime.mjs';
import { assertNativeIsolation } from './qualification-connection.mjs';
import { awaitNativeFirstRun } from './qualification-startup.mjs';
import { connectNativeLive, clearNativeLiveCredential } from './qualification-live-session.mjs';
import { nativeLiveFailure } from './qualification-live-policy.mjs';
import { startVodGateway } from './vod-gateway.mjs';
import { awaitVodAttachment, exerciseVodDecoder } from './vod-decoder-recovery.mjs';
import { pauseThroughUi, pausedThroughUi } from './qualification-pause.mjs';

async function main() {
    if (process.platform !== 'win32') throw new Error('Actual Windows Tauri qualification required');
    const approved = await readQualification(process.env.DUSKCUE_TEST_VOD_NATIVE_MANIFEST);
    const fixture = JSON.parse(await readFile(process.env.DUSKCUE_VOD_CLIENT_FIXTURE, 'utf8'));
    const origin = approved.origins[0];
    const scenario = { origin, token: fixture.token, candidate: fixture.native,
        user: { id: fixture.user, username: `admission-${fixture.user}`, display_name: 'Qualification', role: 'member', capabilities: ['play_media'],
            has_all_library_access: true, active_profile_id: fixture.profile, profile_selection_required: false, locale: 'en' },
        userStorageKey: `duskcue_user:${encodeURIComponent(origin)}` };
    const report = { status: 'in_progress', identifier: approved.identifier, executableSha256: approved.executableSha256,
        checks: {}, limitations: ['This fixture restores a real server session; password login and physical display or assistive-technology speech are separate.'] };
    let runtime;
    let gateway;
    let isolated = false;
    let failure;
    let stage = 'isolation';
    try {
        gateway = await startVodGateway(origin, process.env.DUSKCUE_BACKEND_URL);
        runtime = await launchQualification(approved);
        const { page } = runtime;
        runtime.pageErrors = [];
        page.on('pageerror', (error) => { if (runtime.pageErrors.length < 8) runtime.pageErrors.push(nativeLiveFailure(error, fixture.token, 'page')); });
        page.setDefaultTimeout(15000);
        await awaitNativeFirstRun(page);
        report.checks.isolation = await assertNativeIsolation(page, approved);
        isolated = true;
        stage = 'connect';
        report.checks.connection = await connectNativeLive(page, scenario);
        const region = page.getByRole('region', { name: 'Media player', exact: true });
        const video = region.locator('video');
        await expect(region).toBeVisible();
        stage = 'initial-admission';
        await awaitVodAttachment(video);
        stage = 'initial-decode';
        await expect.poll(() => video.evaluate((element) => element.readyState >= 2 && element.videoWidth > 0), { timeout: 5000 }).toBe(true);
        if (await video.evaluate((element) => element.paused)) await region.getByRole('button', { name: 'Play', exact: true }).click();
        await expect.poll(() => video.evaluate((element) => element.currentTime > .1 && (element.getVideoPlaybackQuality?.().totalVideoFrames || 0) > 0)).toBe(true);
        report.checks.decoded = await video.evaluate((element) => ({ width: element.videoWidth, height: element.videoHeight, frames: element.getVideoPlaybackQuality().totalVideoFrames }));
        stage = 'initial-pause';
        await pauseThroughUi(region, video);
        report.checks.pauseBeforeSeek = await video.evaluate((element) => ({ paused: element.paused, position: element.currentTime }));
        const seek = region.getByRole('slider', { name: 'Seek', exact: true });
        const beforeSeek = await video.evaluate((element) => element.currentSrc);
        await seek.focus(); await page.keyboard.press('ArrowRight'); await page.keyboard.press('ArrowRight'); await page.keyboard.press('Tab');
        stage = 'seek-admission';
        await expect.poll(() => gateway.records.some((record) => record.path.endsWith('/configuration') && record.method === 'POST' && record.status === 202)).toBe(true);
        await awaitVodAttachment(video, beforeSeek);
        stage = 'seek-decode';
        await expect.poll(() => video.evaluate((element) => element.readyState >= 2), { timeout: 5000 }).toBe(true);
        stage = 'seek-paused';
        await pausedThroughUi(region, video);
        const settings = region.getByRole('button', { name: 'Settings', exact: true });
        const beforeQuality = await video.evaluate((element) => element.currentSrc);
        await settings.click();
        await region.getByRole('region', { name: 'Settings', exact: true }).getByRole('button', { name: 'Up to 1.5 Mbps', exact: true }).click();
        stage = 'quality-admission';
        await expect(settings).toBeFocused();
        await expect.poll(() => gateway.records.filter((record) => record.path.endsWith('/configuration') && record.method === 'POST' && record.status === 202).length).toBe(2);
        await awaitVodAttachment(video, beforeQuality);
        stage = 'quality-decode';
        await expect.poll(() => video.evaluate((element) => element.readyState >= 2), { timeout: 5000 }).toBe(true);
        stage = 'quality-paused';
        await pausedThroughUi(region, video);
        await page.screenshot({ path: join(approved.directory, 'vod-command-controls.png') });
        stage = 'cleanup';
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect(region).not.toBeVisible();
        const starts = gateway.records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback-commands');
        const media = gateway.records.filter((record) => record.path.includes('/outputs/') && record.method === 'GET');
        expect(starts).toHaveLength(1);
        expect(starts[0]).toMatchObject({ bearer: true, cookie: false, status: 202 });
        expect(media.some((record) => record.path.endsWith('.m3u8') && record.status === 200)).toBe(true);
        expect(media.some((record) => record.path.endsWith('.m4s') && record.status === 200)).toBe(true);
        expect(media.every((record) => record.bearer && !record.cookie)).toBe(true);
        expect(gateway.records.some((record) => /^\/api\/v1\/(?:stream|transcode|playback\/(?:start|seek|stop|heartbeat))(?:\/|$)/.test(record.path))).toBe(false);
        report.checks.retainedCommand = { starts: starts.length, realBearerHls: true, controls: 2, explicitSettlement: gateway.records.some((record) => record.path.endsWith('/settlement')) };
        stage = 'natural-decoder-recovery';
        await page.goto(`${new URL(page.url()).origin}/play/${fixture.native_recovery.item}?file=${fixture.native_recovery.file}`);
        await expect(region).toBeVisible();
        report.checks.decoderRecovery = await exerciseVodDecoder({ page, region, video,
            recoveries: () => gateway.records.filter((record) => record.action === 'recover'), onStage: (value) => { stage = value; } });
        await region.getByRole('button', { name: 'Close player', exact: true }).click();
        await expect(region).not.toBeVisible();
        report.status = 'passed';
    } catch (error) {
        failure = error;
        report.status = 'failed'; report.error = nativeLiveFailure(error, fixture.token, stage);
        report.failureStage = stage;
        report.transport = gateway?.records.slice(-48);
        report.pageErrors = runtime?.pageErrors;
        if (runtime?.page) report.media = await runtime.page.locator('video').evaluate((element) => ({
            readyState: element.readyState, networkState: element.networkState, errorCode: element.error?.code,
            paused: element.paused, currentTime: element.currentTime, source: element.currentSrc.startsWith('blob:') ? 'blob' : element.currentSrc ? new URL(element.currentSrc).pathname : null,
        })).catch(() => null);
        if (runtime) await runtime.page.screenshot({ path: join(approved.directory, 'vod-command-failure.png') }).catch(() => {});
    } finally {
        if (runtime && isolated) {
            try { report.checks.credentialCleanup = await clearNativeLiveCredential(runtime.page, scenario); }
            catch (error) { failure ||= error; report.status = 'failed'; }
        }
        if (runtime) {
            try { report.cleanup = await stopQualification(runtime); }
            catch (error) { failure ||= error; report.status = 'failed'; }
        }
        if (gateway) await gateway.close();
        await writeFile(join(approved.directory, 'vod-command-result.json'), `${JSON.stringify(report, null, 2)}\n`);
    }
    if (failure) throw new Error(report.error || 'Owned native VOD qualification failed');
    console.log('VOD_NATIVE_RESULT actual_tauri=true real_bearer_hls=true retained_viewer=true cleanup=true');
}
main().catch((error) => { console.error(error.message); process.exitCode = 1; });
