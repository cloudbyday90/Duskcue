// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { spawn } from 'node:child_process';
import { createWriteStream } from 'node:fs';
import { writeFile } from 'node:fs/promises';
import { Transform } from 'node:stream';
import { finished } from 'node:stream/promises';
import { stopOwnedProcess } from './owned-process.mjs';
import { cleanupOwnedContainers } from './owned-containers.mjs';
import { evaluateLinuxHeadroom, readLinuxSample } from './linux-engine-headroom.mjs';

export const COMPILER_OUTPUT_LIMIT = 32 * 1024 ** 2;
export const COMPILER_DEADLINE_MS = 30 * 60 * 1000;

export function createOutputBudget(limit = COMPILER_OUTPUT_LIMIT) {
    if (!Number.isSafeInteger(limit) || limit <= 0) throw new Error('The compiler output limit is invalid.');
    let bytes = 0;
    return { get bytes() { return bytes; }, consume(size) { if (!Number.isSafeInteger(size) || size < 0 || bytes + size > limit) throw new Error('The compiler exceeded its bounded log size.'); bytes += size; } };
}

export async function runCompilerCommand(workspace, args, owner, engine, initialSample, recordSample) {
    const log = createWriteStream(`${owner.directory}/compiler.log`, { flags: 'wx', highWaterMark: 65536 });
    const budget = createOutputBudget();
    const child = spawn('docker.exe', args, { cwd: workspace, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    child.startedAt = Date.now();
    let closed = false;
    let failure;
    let stopping;
    let latest = initialSample;
    let polling;
    let streams = [];
    const outcome = new Promise((resolve, reject) => {
        child.once('error', reject);
        child.once('close', (code, signal) => { closed = true; resolve({ code, signal }); });
    });
    outcome.catch(() => {});
    const stop = (message) => {
        failure ||= message;
        if (closed || stopping) return;
        for (const { source, bounded } of streams) { source.unpipe(bounded); bounded.unpipe(log); bounded.destroy(); source.resume(); }
        stopping = Promise.allSettled([cleanupOwnedContainers(owner.marker, owner.resourceId), stopOwnedProcess(child)]).then((results) => {
            const rejected = results.find((result) => result.status === 'rejected');
            return writeFile(`${owner.directory}/stop-evidence.json`, `${JSON.stringify({ reason: message, childPid: child.pid, startedAt: child.startedAt, latestLinuxSample: latest, removedOwnedContainers: results[0].status === 'fulfilled' ? results[0].value : [], cleanupErrors: results.filter((result) => result.status === 'rejected').map((result) => result.reason.message) }, null, 2)}\n`, 'utf8').then(() => { if (rejected) throw rejected.reason; });
        });
        stopping.catch(() => {});
    };
    log.on('error', () => stop('The compiler log could not be written.'));
    streams = [child.stdout, child.stderr].map((source) => {
        const bounded = new Transform({ highWaterMark: 65536, transform(chunk, encoding, callback) {
            try { budget.consume(chunk.length); callback(null, chunk); }
            catch (error) { stop(error.message); callback(error); }
        } });
        bounded.on('error', () => {});
        source.pipe(bounded).pipe(log, { end: false });
        return { source, bounded };
    });
    const interrupted = () => stop('The owned compiler was interrupted.');
    process.on('SIGINT', interrupted);
    process.on('SIGTERM', interrupted);
    const deadline = setTimeout(() => stop('The owned compiler exceeded its 30-minute deadline.'), COMPILER_DEADLINE_MS);
    const watchdog = setInterval(() => {
        if (closed || stopping) return;
        const decision = evaluateLinuxHeadroom(latest, initialSample, { preflight: false });
        if (!decision.allowed) { stop(decision.reason); return; }
        if (polling) return;
        polling = readLinuxSample(engine).then(async (sample) => {
            latest = sample;
            await recordSample(sample);
            const current = evaluateLinuxHeadroom(sample, initialSample, { preflight: false });
            if (!current.allowed) stop(current.reason);
        }).catch((error) => stop(error.message)).finally(() => { polling = null; });
    }, 2000);
    try {
        const result = await outcome;
        if (stopping) await stopping;
        if (failure) throw new Error(failure);
        if (result.code !== 0 || result.signal) throw new Error(`The Linux compiler container failed (${result.code ?? result.signal}).`);
        return { ...result, logBytes: budget.bytes };
    } finally {
        clearTimeout(deadline);
        clearInterval(watchdog);
        process.off('SIGINT', interrupted);
        process.off('SIGTERM', interrupted);
        if (!closed && child.pid) { await stopOwnedProcess(child); await outcome; }
        if (polling) await polling;
        await Promise.all(streams.map(({ bounded }) => finished(bounded).catch((error) => { if (!failure) throw error; })));
        for (const { source, bounded } of streams) { source.unpipe(bounded); bounded.unpipe(log); bounded.destroy(); }
        log.end();
        let finishTimer;
        try { await Promise.race([finished(log), new Promise((_, reject) => { finishTimer = setTimeout(() => reject(new Error('The bounded compiler log did not finish writing.')), 5000); })]); }
        finally { clearTimeout(finishTimer); }
        if (failure) throw new Error(failure);
    }
}
