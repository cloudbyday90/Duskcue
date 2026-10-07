/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { mkdir, access, appendFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { randomUUID } from 'node:crypto';
import { evaluatePreflight, evaluateEmergency, GiB } from './policy.mjs';
import { PRESETS, workloadEnvironment, validateExtraArguments } from './presets.mjs';
import { acquireWorkflowLock } from './workflow-lock.mjs';
import { stopOwnedProcess, ownedProcessBytes } from './owned-process.mjs';
import { cleanupOwnedContainers } from './owned-containers.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const [workload, ...extraArgs] = process.argv.slice(2);
const preset = PRESETS[workload];
let child;
let sampler;
let lock;
let latest;
let watchdog;
let stopping;
let finished = false;
let logQueue = Promise.resolve();
let logPath;
const resourceId = randomUUID();
let containerMarker;
let firstSample;
let initialTimeout;
let samplingFailure;
let cancelled;
let pendingWrites = 0;
let peak = { committedBytes: 0, privateBytes: 0, workingSetBytes: 0, minAvailableBytes: Infinity };

function record(entry) {
    if (pendingWrites >= 4) { samplingFailure = 'The memory evidence writer is stalled.'; return; }
    pendingWrites += 1;
    logQueue = logQueue.then(() => appendFile(logPath, `${JSON.stringify({ timestamp: new Date().toISOString(), ...entry })}\n`, 'utf8')).finally(() => { pendingWrites -= 1; });
    logQueue.catch(() => { samplingFailure = 'Memory evidence could not be written.'; });
}

function observe(sample) {
    latest = sample;
    const decision = evaluateEmergency(sample);
    const owned = ownedProcessBytes(sample, process.pid);
    peak = {
        committedBytes: Math.max(peak.committedBytes, sample.committedBytes || 0),
        privateBytes: Math.max(peak.privateBytes, owned.privateBytes),
        workingSetBytes: Math.max(peak.workingSetBytes, owned.workingSetBytes),
        minAvailableBytes: Math.min(peak.minAvailableBytes, sample.availablePhysicalBytes ?? Infinity),
    };
    record({ event: 'sample', metrics: decision.metrics || null, owned, error: sample.error || null });
    firstSample?.(sample);
    firstSample = null;
    if (child && !finished && !decision.allowed) abort('Memory reserve was exhausted during the owned workflow.', decision);
}

function abort(message, decision = null) {
    if (stopping || finished || !child) return;
    record({ event: 'stopping', message, decision });
    console.error(message);
    stopping = stopOwnedProcess(child).catch((error) => { console.error(error.message); process.exitCode = 1; });
}

function interrupt(message) {
    cancelled = message;
    abort(message);
}

async function main() {
    if (!preset) throw new Error(`Choose a workload: ${Object.keys(PRESETS).join(', ')}.`);
    validateExtraArguments(extraArgs);
    if (process.platform !== 'win32') throw new Error('This resource wrapper measures Windows commit. Use the normal repository commands on other platforms.');
    const directory = join(workspace, '.cache', 'testing-memory');
    await mkdir(directory, { recursive: true });
    logPath = join(directory, `${new Date().toISOString().replace(/[:.]/g, '-')}-${randomUUID()}.jsonl`);
    containerMarker = join(directory, `${resourceId}.containers.jsonl`);
    lock = await acquireWorkflowLock(join(directory, 'active.json'), workload);
    record({ event: 'started', workload, resourceId, ownerPid: process.pid, policy: preset.policy });
    const initial = new Promise((resolve, reject) => {
        firstSample = resolve;
        initialTimeout = setTimeout(() => reject(new Error('No current Windows memory sample was received.')), 15000);
    });
    sampler = spawn('pwsh.exe', ['-NoProfile', '-File', join(workspace, 'scripts', 'testing-memory', 'windows-sampler.ps1')], { cwd: workspace, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    const lines = createInterface({ input: sampler.stdout });
    lines.on('line', (line) => {
        try { observe(JSON.parse(line)); }
        catch { samplingFailure = 'The Windows memory sample was invalid.'; }
    });
    sampler.on('error', () => { samplingFailure = 'The Windows memory sampler could not start.'; });
    sampler.on('exit', () => { if (!finished) samplingFailure = 'The Windows memory sampler exited.'; });
    sampler.stderr.resume();
    const sample = await initial;
    clearTimeout(initialTimeout);
    if (cancelled) throw new Error(cancelled);
    const preflight = evaluatePreflight(sample, preset.policy);
    if (!preflight.allowed) {
        record({ event: 'blocked', decision: preflight });
        console.error(`Workflow held: ${preflight.reasons.map((reason) => reason.message).join(' ')}`);
        if (preflight.metrics) console.error(`Available RAM ${(sample.availablePhysicalBytes / GiB).toFixed(2)} GiB; commit ${(sample.committedBytes / GiB).toFixed(2)}/${(sample.commitLimitBytes / GiB).toFixed(2)} GiB.`);
        process.exitCode = 75;
        return;
    }
    let executable = preset.command;
    let args = [...preset.args, ...extraArgs];
    if (!executable) {
        executable = process.execPath;
        const npm = join(dirname(process.execPath), 'node_modules', 'npm', 'bin', 'npm-cli.js');
        await access(npm);
        args = [npm, ...args];
    } else if (executable === 'node') executable = process.execPath;
    if (cancelled || samplingFailure) throw new Error(cancelled || samplingFailure);
    const currentPreflight = evaluatePreflight(latest, preset.policy);
    if (!currentPreflight.allowed) {
        record({ event: 'blocked_before_spawn', decision: currentPreflight });
        process.exitCode = 75;
        console.error('Workflow held because memory headroom changed before process startup.');
        return;
    }
    const startedAt = Date.now();
    child = spawn(executable, args, { cwd: workspace, env: { ...workloadEnvironment(preset, process.env), DUSKCUE_TEST_RESOURCE_ID: resourceId, DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE: containerMarker }, windowsHide: true, stdio: 'inherit' });
    child.startedAt = startedAt;
    const result = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => resolve({ code, signal })); });
    result.catch(() => {});
    if (child.pid) await lock.childStarted(child.pid);
    record({ event: 'child_started', childPid: child.pid, cargoJobs: 2, rustTestThreads: 1, heapMiB: preset.heapMiB || null });
    watchdog = setInterval(() => {
        const decision = evaluateEmergency(latest);
        if (samplingFailure || !decision.allowed) abort(samplingFailure || 'Memory telemetry was lost or the memory reserve was exhausted.', decision);
    }, 2000);
    const outcome = await result;
    finished = true;
    clearTimeout(initialTimeout);
    if (stopping) await stopping;
    process.exitCode = cancelled ? 130 : stopping ? 76 : outcome.code ?? 1;
    record({ event: 'finished', outcome, stoppedForMemory: Boolean(stopping) && !cancelled, interrupted: Boolean(cancelled), peak });
}

process.on('SIGINT', () => interrupt('The owned testing workflow was interrupted.'));
process.on('SIGTERM', () => interrupt('The owned testing workflow was terminated.'));

try { await main(); }
catch (error) { console.error(error.message); process.exitCode = 1; if (child && !finished) await stopOwnedProcess(child).catch(() => {}); }
finally {
    finished = true;
    clearInterval(watchdog);
    try { const removed = await cleanupOwnedContainers(containerMarker, resourceId); if (removed.length && logPath) record({ event: 'containers_cleaned', count: removed.length }); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
    sampler?.kill();
    let evidenceTimeout;
    await Promise.race([logQueue, new Promise((_, reject) => { evidenceTimeout = setTimeout(() => reject(new Error('Memory evidence could not finish writing.')), 5000); })]).catch((error) => { console.error(error.message); process.exitCode = 1; });
    clearTimeout(evidenceTimeout);
    if (!child?.pid || child.exitCode !== null || child.signalCode !== null) await lock?.release().catch((error) => { console.error(error.message); process.exitCode = 1; });
    else { console.error('The owned command is still live; the resource lock was preserved.'); process.exitCode = 1; }
    if (logPath) console.log(`Memory evidence: ${logPath}`);
}
