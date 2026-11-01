// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput } from './android-evidence.mjs';
import { portAvailable } from './android-owner.mjs';
import { captureAndroidFailure } from './android-diagnostics.mjs';
import { androidStageBudget } from './android-stage-budget.mjs';
import { bootstrapAndroidDriver } from './android-driver-bootstrap.mjs';
import { androidDriverInputs } from './android-driver-inputs.mjs';

const packageName = 'com.duskcue.mobile.qualification';

async function privateBootstrap(owner, bytes, budget) {
    if (bytes.length > 4096) throw new Error('Private bootstrap exceeds four KiB.');
    const timeoutMs = Math.min(15000, budget.remaining(true));
    const child = spawn(owner.adb, ['-P', String(owner.adbPort), '-s', owner.serial, 'shell', '-T',
        `run-as ${packageName} sh -c 'umask 077; mkdir -p no_backup; cat > no_backup/playback-bootstrap.json'`], {
        env: owner.env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    });
    let size = 0, failed = false, force;
    const stop = () => { failed = true; child.kill(); force ||= setTimeout(() => child.kill('SIGKILL'), 1000); };
    const result = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal })); });
    result.catch(() => {});
    for (const stream of [child.stdout, child.stderr]) stream.on('data', (data) => {
        try { budget.consume(data.length); size += data.length; if (size > 65536) throw new Error('Bootstrap output exceeded its bound.'); }
        catch { stop(); }
    });
    child.stdin.on('error', stop);
    const deadline = setTimeout(stop, timeoutMs);
    const cancel = stop;
    budget.signal.addEventListener('abort', cancel, { once: true });
    if (budget.signal.aborted) cancel();
    child.stdin.end(bytes);
    try { const exit = await result; if (failed || exit.code !== 0 || exit.signal) throw new Error('Owned private bootstrap transfer failed.'); }
    finally { clearTimeout(deadline); clearTimeout(force); budget.signal.removeEventListener('abort', cancel); }
}

export async function runAndroidStage(workspace, owner, { fixture, sdk, dart, driverInputs, directory, onPhase }) {
    const remaining = fixture.native_deadline - Date.now();
    if (!Number.isSafeInteger(remaining) || remaining <= 0) throw new Error('Shared native suite deadline expired.');
    const port = await portAvailable();
    const budget = androidStageBudget(fixture.native_deadline);
    let background;
    let backgroundError;
    let execution;
    let driverFailure;
    let bootstrap;
    try {
        await writeFile(join(directory, 'vm-port.json'), JSON.stringify({ port, bootId: owner.bootId }));
        if ((await androidDriverInputs(workspace, sdk, dart)).sha256 !== driverInputs.sha256) throw new Error('Android SDK driver inputs changed before its stage.');
        await privateBootstrap(owner, Buffer.from(JSON.stringify({ ...fixture, vm_service_port: port })), budget);
        bootstrap = await bootstrapAndroidDriver(owner, { fixture, directory, port, budget,
            onPhase(line) {
                onPhase?.(line);
                if (fixture.scenario !== 'baseline' || !line.endsWith('ANDROID_PHASE background')) return;
                if (background) throw new Error('Duplicate native background phase.');
                background = (async () => {
                    const command = (args) => owner.command(args, { timeoutMs: Math.min(15000, budget.remaining()), bytes: 65536, outputBudget: budget, abortSignal: budget.signal });
                    await command(['shell', 'input', 'keyevent', 'KEYCODE_HOME']);
                    await delay(3000);
                    await command(['shell', 'am', 'start', '-W', '-n', `${packageName}/com.duskcue.mobile.QualificationActivity`]);
                })().catch((error) => { backgroundError = error; budget.fail(error); });
            },
        });
        const env = { ...owner.env, FLUTTER_ROOT: sdk, CI: 'true',
                FLUTTER_SUPPRESS_ANALYTICS: 'true', DART_SUPPRESS_ANALYTICS: 'true', VM_SERVICE_URL: bootstrap.uri,
                DUSKCUE_ANDROID_SCENARIO: fixture.scenario, DUSKCUE_ANDROID_EVIDENCE: directory };
        delete env.FLUTTER_WEB_TEST;
        execution = await runBounded(dart, ['--old_gen_heap_size=1536', `--packages=${driverInputs.config}`, 'test_driver/vod_commands.dart'], {
            cwd: join(workspace, 'clients/mobile'), env,
            timeoutMs: budget.remaining(), outputBudget: budget, abortSignal: budget.signal,
            log: join(directory, 'sdk-driver.log'), redact: [fixture.token, fixture.peer_token, fixture.other_token].filter(Boolean), sanitize: redactAndroidOutput,
        });
    } catch (error) { driverFailure = error; }
    await background;
    const failures = [driverFailure, backgroundError, budget.failure].filter(Boolean);
    if (execution && (execution.code !== 0 || execution.signal)) failures.unshift(new Error(`Actual Android ${fixture.scenario} failed; inspect the bounded owned log.`));
    try { await bootstrap?.close(); } catch (error) { failures.push(error); }
    if (budget.failure && !failures.includes(budget.failure)) failures.push(budget.failure);
    budget.close();
    await writeFile(join(directory, 'stage-budget.json'), JSON.stringify({ bytes: budget.bytes, deadline: budget.deadline, bootstrapDeadline: budget.bootstrapDeadline, failed: failures.length !== 0 }));
    if (failures.length) {
        try { await captureAndroidFailure(owner, directory, fixture); }
        catch (error) { failures.push(error); }
        throw new AggregateError(failures, 'Android driver or owned debug cleanup failed.');
    }
    if (execution.code !== 0 || execution.signal) throw new Error(`Actual Android ${fixture.scenario} failed; inspect the bounded owned log.`);
    if (fixture.scenario === 'baseline' && !background) throw new Error('Actual Android OS background phase was not observed.');
    if (await owner.command(['shell', 'cat', '/proc/sys/kernel/random/boot_id']) !== owner.bootId) throw new Error('Owned Android guest rebooted during its stage.');
    const resultBytes = await readFile(join(directory, 'native-result.json'));
    if (resultBytes.length > 4096) throw new Error('Native result exceeded its evidence bound.');
    const result = JSON.parse(resultBytes);
    if (result.scenario !== fixture.scenario) throw new Error('Native scenario attribution differs.');
    const required = fixture.scenario.startsWith('relay-')
        ? ['same_viewer', 'confirmed_cleanup', ...(fixture.scenario === 'relay-boundary' ? ['eight_socket_gate', 'native_backpressure'] : ['generated_outputs', 'three_authored_frames', 'explicit_quality_changes'])]
        : fixture.scenario.startsWith('scope-')
        ? ['real_provider', 'old_output_refused', 'no_replay', 'confirmed_cleanup', ...(fixture.scenario === 'scope-profile' ? ['profile_cleanup_only', 'http_cancelled'] : ['unsent_cleared', 'native_barrier', 'strong_scope_clear'])]
        : fixture.scenario === 'baseline'
        ? ['protocol_two', 'decoded_authored_frame', 'same_viewer', 'paused_seek', 'paused_tracks_quality', 'explicit_background_play', 'confirmed_cleanup']
        : fixture.scenario === 'worker-recovery'
            ? ['native_checkpoint', 'automatic_worker_recovery', 'negative_then_positive', 'same_viewer', 'preserved_configuration', 'one_recovery', 'confirmed_cleanup']
        : fixture.scenario === 'lost-outcomes'
            ? ['lost_start_reconciled', 'lost_change_reconciled', 'lost_renew_reconciled', 'lost_withdraw_settled', 'same_viewer', 'paused_intent', 'confirmed_cleanup']
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
