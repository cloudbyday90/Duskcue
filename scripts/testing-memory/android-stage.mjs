// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput } from './android-evidence.mjs';

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

export async function runAndroidStage(workspace, owner, { fixture, artifact, sdk, dart, snapshot, directory }) {
    await privateBootstrap(owner, Buffer.from(JSON.stringify(fixture)));
    const execution = await runBounded(dart, ['--old_gen_heap_size=1536', snapshot, '--suppress-analytics', 'drive',
        '--no-pub', '--debug', '--keep-app-running', '--flavor', 'qualification', '-d', owner.serial,
        '--driver=test_driver/vod_commands.dart', '--target=integration_test/vod_commands_test.dart',
        `--use-application-binary=${artifact}`], {
        cwd: join(workspace, 'clients/mobile'), env: { ...owner.env, FLUTTER_ROOT: sdk, CI: 'true',
            FLUTTER_SUPPRESS_ANALYTICS: 'true', DART_SUPPRESS_ANALYTICS: 'true',
            DUSKCUE_ANDROID_SCENARIO: fixture.scenario, DUSKCUE_ANDROID_EVIDENCE: directory },
        timeoutMs: 240000, bytes: 16777216, log: join(directory, 'flutter-drive.log'), redact: [fixture.token], sanitize: redactAndroidOutput,
    });
    if (execution.code !== 0 || execution.signal) throw new Error(`Actual Android ${fixture.scenario} failed; inspect the bounded owned log.`);
    const resultBytes = await readFile(join(directory, 'native-result.json'));
    if (resultBytes.length > 4096) throw new Error('Native result exceeded its evidence bound.');
    const result = JSON.parse(resultBytes);
    if (result.scenario !== fixture.scenario) throw new Error('Native scenario attribution differs.');
    const required = fixture.scenario === 'baseline'
        ? ['protocol_two', 'decoded_authored_frame', 'same_viewer', 'paused_seek', 'explicit_background_play', 'confirmed_cleanup']
        : fixture.scenario.endsWith('-retain')
            ? ['journal_read_back', 'namespaces_intact', 'retained_for_process_death']
            : fixture.scenario === 'corrupt-refuse'
                ? ['corruption_refused', 'close_preserved_ciphertext', 'namespaces_intact']
                : ['cleanup_only_restore', 'namespaces_intact', 'confirmed_cleanup'];
    for (const key of required) if (result[key] !== true) throw new Error('Required actual Android result is missing.');
    if (fixture.scenario.endsWith('-retain') && !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(result.start)) {
        throw new Error('Retained Android Start is not a UUIDv7.');
    }
    await owner.command(['shell', 'am', 'force-stop', packageName]);
    if (await owner.command(['shell', `pidof ${packageName} || true`])) throw new Error('Cold restart did not observe owned application exit.');
    return result;
}
