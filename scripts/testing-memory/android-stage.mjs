// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput, androidTcpMapping } from './android-evidence.mjs';
import { portAvailable, listening } from './android-owner.mjs';

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
    if (await owner.command(['shell', 'cat', '/proc/sys/kernel/random/boot_id']) !== owner.bootId) throw new Error('Owned Android guest rebooted between stages.');
    const port = await portAvailable();
    await writeFile(join(directory, 'vm-port.json'), JSON.stringify({ port, bootId: owner.bootId }));
    await privateBootstrap(owner, Buffer.from(JSON.stringify({ ...fixture, vm_service_port: port })));
    let background;
    let backgroundError;
    let execution;
    let driverFailure;
    try {
        execution = await runBounded(dart, ['--old_gen_heap_size=1536', snapshot, '--suppress-analytics', 'drive',
            '--no-pub', '--debug', '--no-dds', '--keep-app-running', '--flavor', 'qualification', '-d', owner.serial,
            '--driver=test_driver/vod_commands.dart', '--target=integration_test/vod_commands_test.dart',
            `--host-vmservice-port=${port}`, `--device-vmservice-port=${port}`,
            `--use-application-binary=${artifact}`], {
            cwd: join(workspace, 'clients/mobile'), env: { ...owner.env, FLUTTER_ROOT: sdk, CI: 'true',
                FLUTTER_SUPPRESS_ANALYTICS: 'true', DART_SUPPRESS_ANALYTICS: 'true',
                DUSKCUE_ANDROID_SCENARIO: fixture.scenario, DUSKCUE_ANDROID_EVIDENCE: directory },
            timeoutMs: 240000, bytes: 16777216, log: join(directory, 'flutter-drive.log'), redact: [fixture.token], sanitize: redactAndroidOutput,
            onLine(line) {
                if (fixture.scenario !== 'baseline' || !line.endsWith('ANDROID_PHASE background')) return;
                if (background) throw new Error('Duplicate native background phase.');
                background = (async () => {
                    await owner.command(['shell', 'input', 'keyevent', 'KEYCODE_HOME']);
                    await delay(3000);
                    await owner.command(['shell', 'am', 'start', '-W', '-n', `${packageName}/com.duskcue.mobile.QualificationActivity`]);
                })().catch((error) => { backgroundError = error; });
            },
        });
    } catch (error) { driverFailure = error; }
    await background;
    const failures = [driverFailure, backgroundError].filter(Boolean);
    if (execution && (execution.code !== 0 || execution.signal)) failures.unshift(new Error(`Actual Android ${fixture.scenario} failed; inspect the bounded owned log.`));
    try {
        const before = await owner.command(['forward', '--list']);
        if (androidTcpMapping(before, port, owner.serial)) await owner.command(['forward', '--remove', `tcp:${port}`]);
        const after = await owner.command(['forward', '--list']);
        const present = androidTcpMapping(after, port, owner.serial);
        await writeFile(join(directory, 'debug-mappings.json'), JSON.stringify({ before, after, port }));
        const deadline = Date.now() + 5000;
        while (await listening(port) && Date.now() < deadline) await delay(50);
        if (present || await listening(port)) throw new Error('Owned Flutter debug forwarding remains.');
    } catch (error) { failures.push(error); }
    if (failures.length) throw new AggregateError(failures, 'Android driver or owned debug cleanup failed.');
    if (execution.code !== 0 || execution.signal) throw new Error(`Actual Android ${fixture.scenario} failed; inspect the bounded owned log.`);
    if (fixture.scenario === 'baseline' && !background) throw new Error('Actual Android OS background phase was not observed.');
    if (await owner.command(['shell', 'cat', '/proc/sys/kernel/random/boot_id']) !== owner.bootId) throw new Error('Owned Android guest rebooted during its stage.');
    const resultBytes = await readFile(join(directory, 'native-result.json'));
    if (resultBytes.length > 4096) throw new Error('Native result exceeded its evidence bound.');
    const result = JSON.parse(resultBytes);
    if (result.scenario !== fixture.scenario) throw new Error('Native scenario attribution differs.');
    const required = fixture.scenario === 'baseline'
        ? ['protocol_two', 'decoded_authored_frame', 'same_viewer', 'paused_seek', 'paused_tracks_quality', 'explicit_background_play', 'confirmed_cleanup']
        : fixture.scenario.endsWith('-retain')
            ? ['journal_read_back', 'namespaces_intact', 'retained_for_process_death']
            : fixture.scenario === 'corrupt-refuse'
                ? ['corruption_refused', 'close_preserved_ciphertext', 'namespaces_intact']
                : ['cleanup_only_restore', 'namespaces_intact', 'confirmed_cleanup'];
    for (const key of required) if (result[key] !== true) throw new Error('Required actual Android result is missing.');
    if (fixture.scenario === 'cold-restore' && result.missed_lease_refused_delivery !== true) throw new Error('Expired Android lease still permitted delivery.');
    if (fixture.scenario.endsWith('-retain') && !/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(result.start)) {
        throw new Error('Retained Android Start is not a UUIDv7.');
    }
    await owner.command(['shell', 'am', 'force-stop', packageName]);
    if (await owner.command(['shell', `pidof ${packageName} || true`])) throw new Error('Cold restart did not observe owned application exit.');
    return result;
}
