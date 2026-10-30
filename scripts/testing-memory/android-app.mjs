// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, realpath, writeFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { hashFile } from '../qualification/source.mjs';
import { flutterInputs } from './flutter-inputs.mjs';
import { installedAndroidApk, androidTcpMapping } from './android-evidence.mjs';

import { runAndroidStage } from './android-stage.mjs';
import { runAndroidFaultStage } from './android-fault-stage.mjs';

const packageName = 'com.duskcue.mobile.qualification';

export async function runAndroidApplication(workspace, owner, fixture) {
    const inputs = await flutterInputs(workspace);
    const receipt = JSON.parse(await readFile(join(workspace, '.cache/android-playback/apk-receipt.json'), 'utf8'));
    const artifact = join(workspace, receipt.artifact);
    if (receipt.source !== inputs.sha256 || receipt.apk !== await hashFile(artifact)) throw new Error('Rebuild the qualification APK for current inputs.');
    const properties = await readFile(join(workspace, 'clients/mobile/android/local.properties'), 'utf8');
    const sdk = await realpath(properties.match(/^flutter\.sdk=(.+)$/m)[1].trim().replaceAll('\\\\', '\\'));
    const dart = join(sdk, 'bin/cache/dart-sdk/bin/dart.exe');
    const snapshot = join(sdk, 'bin/cache/flutter_tools.snapshot');
    if (await hashFile(dart) !== receipt.tools.dart || await hashFile(snapshot) !== receipt.tools.flutter) throw new Error('APK and driver toolchains differ.');
    const origin = new URL(fixture.origin);
    if (origin.origin !== fixture.origin || origin.protocol !== 'http:' || origin.hostname !== '127.0.0.1' || !origin.port) throw new Error('Use the owned loopback backend.');
    let installed = false, reversed = false, failure;
    try {
        const installation = await owner.command(['install', '--no-streaming', artifact], { timeoutMs: 60000 });
        installed = true;
        await writeFile(join(owner.directory, 'install.log'), installation);
        if (installation.replaceAll('\r', '').split('\n').at(-1) !== 'Success') throw new Error('Owned APK install failed.');
        const packagePath = installedAndroidApk(await owner.command(['shell', 'pm', 'path', packageName]), packageName);
        const digest = (await owner.command(['shell', 'sha256sum', packagePath])).split(/\s+/)[0];
        await writeFile(join(owner.directory, 'apk-identity.json'), JSON.stringify({ packageName, packagePath, sha256: digest }));
        if (digest !== receipt.apk) throw new Error('Installed APK identity differs from the verified artifact.');
        if (await owner.command(['shell', 'pm', 'clear', packageName]) !== 'Success') throw new Error('Owned app state did not clear.');
        await owner.command(['reverse', '--no-rebind', `tcp:${origin.port}`, `tcp:${origin.port}`]); reversed = true;
        const stages = [];
        let expectedStart;
        for (const scenario of ['baseline', 'cold-retain', 'cold-restore', 'corrupt-retain', 'corrupt-refuse', 'corrupt-restore']) {
            const directory = join(owner.directory, scenario);
            await mkdir(directory);
            const result = await runAndroidStage(workspace, owner, {
                fixture: { ...fixture, scenario, ...(expectedStart ? { expected_start: expectedStart } : {}) },
                artifact, sdk, dart, snapshot, directory,
            });
            stages.push(result);
            if (scenario === 'cold-retain') await delay(65000);
            if (scenario.endsWith('-retain')) expectedStart = result.start;
            if (scenario.endsWith('-restore')) {
                if (result.start !== expectedStart) throw new Error('Cold journal changed the retained Start identity.');
                expectedStart = undefined;
            }
        }
        stages.push(await runAndroidFaultStage(workspace, owner, { fixture, artifact, sdk, dart, snapshot }));
        const workerDirectory = join(owner.directory, 'worker-recovery');
        await mkdir(workerDirectory);
        stages.push(await runAndroidStage(workspace, owner, { fixture: { ...fixture, scenario: 'worker-recovery' }, artifact, sdk, dart, snapshot, directory: workerDirectory }));
        if ((await flutterInputs(workspace)).sha256 !== inputs.sha256) throw new Error('Android sources changed during execution.');
        return { stages, apk: receipt.apk, source: inputs.sha256 };
    } catch (error) { failure = error; throw error; }
    finally {
        const errors = [];
        if (installed) {
            try {
                await owner.command(['shell', 'am', 'force-stop', packageName]);
                if (await owner.command(['shell', `pidof ${packageName} || true`])) throw new Error('Owned application process remains.');
                if (await owner.command(['shell', 'pm', 'clear', packageName]) !== 'Success') throw new Error('Owned app-private state clearing was not confirmed.');
                if (await owner.command(['uninstall', packageName]) !== 'Success') throw new Error('Owned app credentials/bootstrap/journal removal was not confirmed.');
            } catch (error) { errors.push(error); }
        }
        if (reversed) try {
            const key = `tcp:${origin.port}`;
            if (androidTcpMapping(await owner.command(['reverse', '--list']), Number(origin.port))) await owner.command(['reverse', '--remove', key]);
            if (androidTcpMapping(await owner.command(['reverse', '--list']), Number(origin.port))) throw new Error('Owned backend reverse remains.');
        } catch (error) { errors.push(error); }
        if (errors.length) throw new AggregateError([...(failure ? [failure] : []), ...errors], 'Android app cleanup is unconfirmed.');
    }
}
