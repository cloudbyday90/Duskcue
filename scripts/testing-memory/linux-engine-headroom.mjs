// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { execFile } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { GiB } from './policy.mjs';

export const LINUX_COMPILER_LIMITS = Object.freeze({ memoryBytes: 4 * GiB, reserveBytes: 1.5 * GiB, maxSampleAgeMs: 10000 });

export class MetadataReadError extends Error {
    constructor(executable, details = {}) {
        const label = /^[A-Za-z0-9_.-]{1,64}$/.test(executable || '') ? executable : 'metadata-command';
        const context = {
            code: typeof details.code === 'string' && /^[A-Z0-9_]{1,64}$/.test(details.code) || Number.isSafeInteger(details.code) && details.code >= -2147483648 && details.code <= 4294967295 ? details.code : null,
            signal: typeof details.signal === 'string' && /^SIG[A-Z0-9]{1,16}$/.test(details.signal) ? details.signal : null,
            killed: details.killed === true,
            elapsedMs: Number.isFinite(details.elapsedMs) && details.elapsedMs >= 0 && details.elapsedMs <= Number.MAX_SAFE_INTEGER ? Math.round(details.elapsedMs) : null,
        };
        super(`Read-only ${label} metadata was unavailable. ${JSON.stringify(context)}`);
        this.name = 'MetadataReadError';
        this.context = Object.freeze(context);
    }
}

export function boundedMetadata(executable, args) {
    return new Promise((resolve, reject) => {
        const started = performance.now();
        execFile(executable, args, { windowsHide: true, timeout: 5000, maxBuffer: 65536, encoding: 'buffer' }, (error, output) => {
            if (error) reject(new MetadataReadError(executable, { code: error.code, signal: error.signal, killed: error.killed, elapsedMs: performance.now() - started }));
            else resolve(output.toString(output.includes(0) ? 'utf16le' : 'utf8').replace(/\0/g, '').trim());
        });
    });
}

export function validateLocalEngine(context, endpoint, info) {
    if (!/^[A-Za-z0-9_.-]+$/.test(context || '') || !/^npipe:\/\/\/+\.\/pipe\/dockerDesktopLinuxEngine$/i.test(endpoint || '')
        || info?.osType !== 'linux' || info?.name !== 'docker-desktop' || info?.operatingSystem !== 'Docker Desktop'
        || typeof info.id !== 'string' || !info.id || !/microsoft.*WSL2/i.test(info.kernel || '')) throw new Error('The compiler requires the current local Docker Desktop WSL2 Linux engine.');
    return { context, engineId: info.id, kernel: info.kernel, architecture: info.architecture };
}

export function parseLinuxSample(text, engine, observedAt = Date.now()) {
    const total = /^MemTotal:\s+(\d+) kB\s*$/m.exec(text);
    const available = /^MemAvailable:\s+(\d+) kB\s*$/m.exec(text);
    const bootId = /^([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\s*$/m.exec(text)?.[1];
    const kernel = /^Linux version (\S+)/m.exec(text)?.[1];
    if (!total || !available || !bootId || kernel !== engine.kernel || !Number.isFinite(observedAt)) throw new Error('The running WSL memory sample could not be matched to the Docker engine.');
    return { timestamp: new Date(observedAt).toISOString(), context: engine.context, engineId: engine.engineId, kernel, bootId, totalBytes: Number(total[1]) * 1024, availableBytes: Number(available[1]) * 1024 };
}

export function evaluateLinuxHeadroom(sample, expected, { nowMs = Date.now(), preflight = true } = {}) {
    const ageMs = nowMs - Date.parse(sample?.timestamp);
    if (!Number.isFinite(nowMs) || !Number.isFinite(ageMs) || ageMs < -5000 || ageMs > LINUX_COMPILER_LIMITS.maxSampleAgeMs
        || !Number.isSafeInteger(sample?.totalBytes) || !Number.isSafeInteger(sample?.availableBytes)
        || sample.totalBytes <= 0 || sample.availableBytes < 0 || sample.availableBytes > sample.totalBytes
        || !expected || ['context', 'engineId', 'kernel', 'bootId'].some((key) => !expected[key] || sample[key] !== expected[key])) return { allowed: false, reason: 'Linux memory evidence is stale, invalid or belongs to a different engine boot.' };
    const minimumBytes = LINUX_COMPILER_LIMITS.reserveBytes + (preflight ? LINUX_COMPILER_LIMITS.memoryBytes : 0);
    return { allowed: sample.availableBytes >= minimumBytes, reason: sample.availableBytes >= minimumBytes ? null : 'Linux MemAvailable is below the compiler reserve.', availableBytes: sample.availableBytes, minimumBytes, ageMs };
}

async function engineInfo(context, readMetadata = boundedMetadata) {
    return JSON.parse(await readMetadata('docker.exe', ['--context', context, 'info', '--format', '{"id":{{json .ID}},"osType":{{json .OSType}},"operatingSystem":{{json .OperatingSystem}},"name":{{json .Name}},"kernel":{{json .KernelVersion}},"architecture":{{json .Architecture}}}']));
}

export async function readMatchingEngine(engine, readMetadata = boundedMetadata) {
    const matched = validateLocalEngine(engine.context, 'npipe:////./pipe/dockerDesktopLinuxEngine', await engineInfo(engine.context, readMetadata));
    if (matched.engineId !== engine.engineId || matched.kernel !== engine.kernel) throw new Error('The local Docker engine changed during qualification.');
    return matched;
}

export async function readLocalEngine() {
    const context = await boundedMetadata('docker.exe', ['context', 'show']);
    const endpoint = JSON.parse(await boundedMetadata('docker.exe', ['context', 'inspect', context, '--format', '{{json .Endpoints.docker.Host}}']));
    return validateLocalEngine(context, endpoint, await engineInfo(context));
}

export async function readLinuxSample(engine, readMetadata = boundedMetadata) {
    const [running, current] = await Promise.allSettled([
        readMetadata('wsl.exe', ['--list', '--running', '--quiet']),
        readMatchingEngine(engine, readMetadata),
    ]);
    const errors = [];
    if (running.status === 'rejected') errors.push(running.reason);
    else if (!running.value.split(/\r?\n/).map((line) => line.trim()).includes('docker-desktop')) errors.push(new Error('The docker-desktop WSL distribution is not already running; it will not be started by qualification.'));
    if (current.status === 'rejected') errors.push(current.reason);
    if (errors.length) throw errors.find((error) => !(error instanceof MetadataReadError)) || errors[0];
    const observedAt = Date.now();
    const text = await readMetadata('wsl.exe', ['--distribution', 'docker-desktop', '--exec', 'cat', '/proc/meminfo', '/proc/sys/kernel/random/boot_id', '/proc/version']);
    return parseLinuxSample(text, engine, observedAt);
}
