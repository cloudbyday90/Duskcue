// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { execFile } from 'node:child_process';
import { GiB } from './policy.mjs';

export const LINUX_COMPILER_LIMITS = Object.freeze({ memoryBytes: 4 * GiB, reserveBytes: 1.5 * GiB, maxSampleAgeMs: 10000 });

export function boundedMetadata(executable, args) {
    return new Promise((resolve, reject) => {
        execFile(executable, args, { windowsHide: true, timeout: 5000, maxBuffer: 65536, encoding: 'buffer' }, (error, output) => {
            if (error) reject(new Error(`Read-only ${executable} metadata was unavailable.`));
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

async function engineInfo(context) {
    return JSON.parse(await boundedMetadata('docker.exe', ['--context', context, 'info', '--format', '{"id":{{json .ID}},"osType":{{json .OSType}},"operatingSystem":{{json .OperatingSystem}},"name":{{json .Name}},"kernel":{{json .KernelVersion}},"architecture":{{json .Architecture}}}']));
}

export async function readLocalEngine() {
    const context = await boundedMetadata('docker.exe', ['context', 'show']);
    const endpoint = JSON.parse(await boundedMetadata('docker.exe', ['context', 'inspect', context, '--format', '{{json .Endpoints.docker.Host}}']));
    return validateLocalEngine(context, endpoint, await engineInfo(context));
}

export async function readLinuxSample(engine) {
    const running = await boundedMetadata('wsl.exe', ['--list', '--running', '--quiet']);
    if (!running.split(/\r?\n/).map((line) => line.trim()).includes('docker-desktop')) throw new Error('The docker-desktop WSL distribution is not already running; it will not be started by qualification.');
    const current = await engineInfo(engine.context);
    const matched = validateLocalEngine(engine.context, 'npipe:////./pipe/dockerDesktopLinuxEngine', current);
    if (matched.engineId !== engine.engineId || matched.kernel !== engine.kernel) throw new Error('The local Docker engine changed during qualification.');
    const observedAt = Date.now();
    const text = await boundedMetadata('wsl.exe', ['--distribution', 'docker-desktop', '--exec', 'cat', '/proc/meminfo', '/proc/sys/kernel/random/boot_id', '/proc/version']);
    return parseLinuxSample(text, engine, observedAt);
}
