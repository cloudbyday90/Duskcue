// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect } from '@playwright/test';
import { prepareQualification, readQualification } from './qualification-config.mjs';
import { launchQualification, startHealthFixtures, stopQualification } from './qualification-runtime.mjs';
import { installQualificationApi } from './qualification-api.mjs';
import { assertNativeIsolation, clearFixtureCredentials, exerciseConnection, exerciseServerSwitch } from './qualification-connection.mjs';
import { exerciseNativeHls, exerciseNativePlayback, exercisePreferences, releaseNativePlaybackForCleanup } from './qualification-playback.mjs';
import { exerciseNativeNavigation } from './qualification-navigation.mjs';
import { exerciseNativeZoom } from './qualification-zoom.mjs';
import { exerciseNativeEvents } from './qualification-events.mjs';
import { exerciseAutoplayZoom } from './qualification-autoplay-zoom.mjs';
import { captureNativeArtifact, createNativeCaptureInventory } from './qualification-capture.mjs';
import { exerciseNativeBrowsing } from './qualification-browsing.mjs';
import { awaitNativeFirstRun } from './qualification-startup.mjs';

if (process.argv[2] === '--prepare') {
    const manifest = await prepareQualification();
    process.stdout.write(`${JSON.stringify({ manifest: manifest.manifestPath, config: manifest.configPath, executable: manifest.executable, identifier: manifest.identifier }, null, 2)}\n`);
} else if (process.argv[2] === '--run' && process.argv[3]) {
    const manifest = await readQualification(process.argv[3]);
    const result = { startedAt: new Date().toISOString(), identifier: manifest.identifier, executableSha256: manifest.executableSha256, hostedCI: manifest.hostedCI, status: 'in_progress', checks: {}, limitations: ['Backend responses are isolated API fixtures; this is not live database authentication or transcoding qualification.', 'Native menu/tray clicks, notification delivery, assistive-technology speech and OS display scaling require separate direct observation. Engine zoom behavior and screenshots do not establish full WCAG conformance.'] };
    let health;
    let runtime;
    let api;
    let isolated = false;
    let failure;
    const uiEvidence = createNativeCaptureInventory();
    try {
        health = await startHealthFixtures(manifest.origins);
        runtime = await launchQualification(manifest);
        const { page } = runtime;
        result.runtimeBrowserVersion = runtime.browser.version();
        runtime.pageErrors = [];
        page.on('pageerror', (error) => runtime.pageErrors.push({ message: error.message, stack: error.stack }));
        page.setDefaultTimeout(15_000);
        result.startupReadiness = await awaitNativeFirstRun(page);
        result.checks.isolation = await assertNativeIsolation(page, manifest);
        isolated = true;
        api = await installQualificationApi(page, manifest);
        const screenshot = (name) => captureNativeArtifact(page, manifest, uiEvidence, name);
        result.checks.connection = await exerciseConnection(page, manifest, api, health, screenshot);
        result.checks.eventsPrimary = await exerciseNativeEvents(page, api, manifest.origins[0], screenshot, 'primary');
        await screenshot('native-home');
        result.checks.browsing = await exerciseNativeBrowsing(page, api, screenshot);
        result.checks.preferences = await exercisePreferences(page, api, screenshot);
        result.checks.playback = await exerciseNativePlayback(page, runtime, api, screenshot);
        result.checks.hls = await exerciseNativeHls(page, api, screenshot);
        result.checks.navigation = await exerciseNativeNavigation(page, screenshot);
        result.checks.zoom = await exerciseNativeZoom(page, manifest, api, uiEvidence);
        result.checks.autoplayZoom = await exerciseAutoplayZoom(page, manifest, api, uiEvidence, runtime);
        result.checks.serverSwitch = await exerciseServerSwitch(page, manifest, api, health);
        result.checks.eventsSwitched = await exerciseNativeEvents(page, api, manifest.origins[1], screenshot, 'switched');
        expect(api.scenario.unhandledRequests).toEqual([]);
        expect(api.transportErrors).toEqual([]);
        expect(runtime.pageErrors).toEqual([]);
        expect(api.transportRequests.every((request) => request.originAllowed)).toBe(true);
        for (const kind of ['text', 'outline', 'icon']) expect(uiEvidence.contrast.filter((sample) => sample.kind === kind && sample.supported).length, `Native ${kind} contrast must include actual supported samples`).toBeGreaterThan(0);
        result.status = 'passed';
    } catch (error) {
        failure = error;
        result.status = 'failed';
        result.error = error.stack || String(error);
        if (runtime) await runtime.page.screenshot({ path: join(manifest.directory, 'native-failure.png'), fullPage: true }).catch(() => {});
    } finally {
        result.uiEvidence = uiEvidence;
        if (runtime?.page && api && !runtime.page.isClosed()) {
            try { result.playbackCleanup = await releaseNativePlaybackForCleanup(runtime.page, api); }
            catch (error) { result.playbackCleanupError = error.message; failure ||= error; result.status = 'failed'; }
        }
        if (api) result.requests = { nativeHealth: health?.requests || [], api: api.transportRequests, events: api.events.records, transportErrors: api.transportErrors, playback: api.scenario.requests.filter((request) => /^\/playback\/(start|heartbeat|stop|seek)$/.test(request.path)) };
        if (runtime) result.pageErrors = runtime.pageErrors;
        api?.events.dispose();
        if (isolated && runtime && api) {
            try { result.clearedFixtureCredentials = await clearFixtureCredentials(runtime.page, manifest, api); }
            catch (error) { result.cleanupError = error.stack || String(error); failure ||= error; result.status = 'failed'; }
        }
        try { await api?.disposeObservation(); }
        catch (error) { result.observationCleanupError = error.message; failure ||= error; result.status = 'failed'; }
        if (runtime) {
            try { result.processCleanup = await stopQualification(runtime); }
            catch (error) { result.processCleanupError = error.stack || String(error); failure ||= error; result.status = 'failed'; }
        }
        await health?.close();
        result.finishedAt = new Date().toISOString();
        await writeFile(join(manifest.directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    process.stdout.write(`${JSON.stringify({ status: result.status, result: join(manifest.directory, 'result.json'), checks: Object.keys(result.checks) }, null, 2)}\n`);
    if (failure) { process.stderr.write(`${failure.message}\n`); process.exitCode = 1; }
} else {
    process.stderr.write('Usage: node clients/web/tests/native/qualify.mjs --prepare | --run <manifest.json>\n');
    process.exitCode = 2;
}
