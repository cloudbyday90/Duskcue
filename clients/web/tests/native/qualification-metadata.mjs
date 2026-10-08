// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { execFile } from 'node:child_process';
import { win32 } from 'node:path';

export const METADATA_DEADLINE_MS = 10000;
export const METADATA_OUTPUT_BYTES = 131072;
const stages = new Set(['checkout_commit', 'system_graphics', 'ffmpeg_version', 'ffmpeg_encoders']);
const executables = new Set(['git', 'git.exe', 'pwsh.exe', 'ffmpeg.exe']);
const codes = new Set(['ENOENT', 'EACCES', 'EPERM', 'ENOBUFS', 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER', 'ETIMEDOUT', 'ESRCH']);
const signals = new Set(['SIGTERM', 'SIGKILL', 'SIGABRT', 'SIGSEGV', 'SIGINT']);
const markers = new Set(['runtime_registry', 'display', 'session', 'complete']);

export function hostedMetadataFailure(executable, stage, error, elapsedMs, stderr = '') {
    const name = win32.basename(executable).toLowerCase();
    const code = codes.has(error?.code) ? error.code : Number.isSafeInteger(error?.code) && Math.abs(error.code) <= 2147483648 ? error.code : null;
    const signal = signals.has(error?.signal) ? error.signal : null;
    const elapsed = Number.isFinite(elapsedMs) ? Math.max(0, Math.min(300000, Math.round(elapsedMs))) : null;
    const killed = error?.killed === true;
    const marker = (typeof stderr === 'string' ? stderr.slice(-8192) : '').split(/\r?\n/).map((line) => /^tonight-hosted:([a-z_]+)$/.exec(line)?.[1]).filter((value) => markers.has(value)).at(-1) || null;
    const category = code === 'ENOENT' ? 'executable_missing' : ['EACCES', 'EPERM'].includes(code) ? 'access_denied'
        : ['ENOBUFS', 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER'].includes(code) ? 'output_limit'
            : code === 'ETIMEDOUT' || killed && elapsed !== null && elapsed >= METADATA_DEADLINE_MS ? 'deadline'
                : signal || killed ? 'terminated' : typeof code === 'number' ? 'nonzero_exit' : 'unclassified';
    return { executable: executables.has(name) ? name : 'other', stage: stages.has(stage) ? stage : 'metadata', category, elapsedMs: elapsed, killed, code, signal, marker, deadlineMs: METADATA_DEADLINE_MS, outputLimitBytes: METADATA_OUTPUT_BYTES };
}

export function createHostedMetadataReader({ cwd, execute = execFile, now = () => performance.now() }) {
    return (executable, args, stage) => new Promise((resolveOutput, reject) => {
        const started = now();
        const completed = (error, output, stderr) => {
            if (error) {
                const failure = hostedMetadataFailure(executable, stage, error, now() - started, stderr);
                const wrapped = new Error(`Hosted prerequisite metadata was unavailable from ${failure.executable}.`);
                wrapped.metadataFailure = failure;
                reject(wrapped);
            } else resolveOutput(output.trim());
        };
        try { execute(executable, args, { cwd, windowsHide: true, timeout: METADATA_DEADLINE_MS, maxBuffer: METADATA_OUTPUT_BYTES }, completed); }
        catch (error) { completed(error, '', ''); }
    });
}

export async function retainHostedProbe(context, inspect, persist, checkedAt = new Date().toISOString()) {
    const result = { version: 1, kind: 'native-hosted-prerequisites', checkedAt, context, status: 'in_progress' };
    let failure;
    try {
        const observed = await inspect();
        result.prerequisites = observed.prerequisites;
        result.ffmpeg = observed.ffmpeg;
        result.status = 'passed';
    } catch (error) {
        result.status = 'failed';
        result.error = error.message;
        if (error.metadataFailure) result.metadataFailure = error.metadataFailure;
        failure = error;
    }
    await persist(result);
    return { result, failure };
}
