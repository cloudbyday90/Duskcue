// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { readFile, realpath, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';
import { flutterInputs } from './flutter-inputs.mjs';
import { redactAndroidOutput, installedAndroidApk } from './android-evidence.mjs';

const packageName = 'com.duskcue.mobile.qualification';

async function privateBootstrap(owner, bytes) {
    if (bytes.length > 4096) throw new Error('Private bootstrap exceeds four KiB.');
    const child = spawn(owner.adb, ['-s', owner.serial, 'shell', '-T',
        `run-as ${packageName} sh -c 'umask 077; mkdir -p no_backup; cat > no_backup/playback-bootstrap.json'`], {
        env: owner.env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    });
    let size = 0, failed = false;
    const result = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal })); });
    result.catch(() => {});
    for (const stream of [child.stdout, child.stderr]) stream.on('data', (data) => { size += data.length; if (size > 65536) { failed = true; child.kill(); } });
    child.stdin.on('error', () => { failed = true; });
    const deadline = setTimeout(() => { failed = true; child.kill(); }, 15000);
    child.stdin.end(bytes);
    try { const exit = await result; if (failed || exit.code !== 0 || exit.signal) throw new Error('Owned private bootstrap transfer failed.'); }
    finally { clearTimeout(deadline); }
}

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
        await privateBootstrap(owner, Buffer.from(JSON.stringify(fixture)));
        const execution = await runBounded(dart, ['--old_gen_heap_size=1536', snapshot, '--suppress-analytics', 'drive',
            '--no-pub', '--debug', '--keep-app-running', '--flavor', 'qualification', '-d', owner.serial,
            '--driver=test_driver/vod_commands.dart', '--target=integration_test/vod_commands_test.dart',
            `--use-application-binary=${artifact}`], {
            cwd: join(workspace, 'clients/mobile'), env: { ...owner.env, FLUTTER_ROOT: sdk, CI: 'true',
                FLUTTER_SUPPRESS_ANALYTICS: 'true', DART_SUPPRESS_ANALYTICS: 'true', DUSKCUE_ANDROID_EVIDENCE: owner.directory },
            timeoutMs: 240000, bytes: 16777216, log: join(owner.directory, 'flutter-drive.log'), redact: [fixture.token], sanitize: redactAndroidOutput,
        });
        if (execution.code !== 0 || execution.signal) throw new Error('Actual Android test failed; inspect the bounded owned log.');
        const result = JSON.parse(await readFile(join(owner.directory, 'native-result.json'), 'utf8'));
        for (const key of ['protocol_two', 'decoded_authored_frame', 'same_viewer', 'paused_seek', 'explicit_background_play', 'confirmed_cleanup']) {
            if (result[key] !== true) throw new Error('Required actual Android result is missing.');
        }
        if ((await flutterInputs(workspace)).sha256 !== inputs.sha256) throw new Error('Android sources changed during execution.');
        return { result, apk: receipt.apk, source: inputs.sha256 };
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
        if (reversed) try { await owner.command(['reverse', '--remove', `tcp:${origin.port}`]); } catch (error) { errors.push(error); }
        if (errors.length) throw new AggregateError([...(failure ? [failure] : []), ...errors], 'Android app cleanup is unconfirmed.');
    }
}
