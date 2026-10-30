// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { hashFile } from '../qualification/source.mjs';

export function redactAndroidOutput(text) {
    return text.replace(/(?:http|ws):\/\/(?:127\.0\.0\.1|localhost):\d+\/[A-Za-z0-9_=-]{8,44}\/(?:[^\s'"<>)]*)?/g, '[redacted-local-capability-url]');
}

export function androidFailure(error, token, depth = 0) {
    const result = { message: redactAndroidOutput(String(error?.message || 'Android qualification failed').replaceAll(token, '[redacted]')).slice(0, 512) };
    if (depth < 3 && Array.isArray(error?.errors)) result.causes = error.errors.slice(0, 8).map((cause) => androidFailure(cause, token, depth + 1));
    return result;
}

export function validateAndroidHardware(text, guestMiB) {
    const values = new Map();
    for (const line of text.split(/\r?\n/)) {
        const match = line.match(/^(hw\.ramSize|hw\.cpu\.ncore)\s*=\s*(\d+)\s*$/);
        if (!match) continue;
        if (values.has(match[1])) throw new Error('Duplicate actual Android hardware setting.');
        values.set(match[1], Number(match[2]));
    }
    if (values.get('hw.ramSize') !== guestMiB || values.get('hw.cpu.ncore') !== 2) {
        throw new Error('Actual emulator exceeded or differs from the configured guest RAM/CPU budget.');
    }
}

export function validateAndroidAvd(response, name) {
    if (typeof response !== 'string' || response.length > 1024) throw new Error('Invalid emulator identity response.');
    const lines = response.replaceAll('\r', '').trim().split('\n');
    if (lines.length !== 2 || lines[0] !== name || lines[1] !== 'OK') {
        throw new Error('Connected emulator AVD identity differs from this owner.');
    }
}

export function installedAndroidApk(response, packageName) {
    const match = response.match(/^package:(\/data\/app\/([A-Za-z0-9_~+=.-]+)\/([A-Za-z0-9_~+=.-]+)\/base\.apk)$/);
    if (!match || match[2] === '..' || !match[3].startsWith(`${packageName}-`)) throw new Error('Unexpected installed APK path.');
    return match[1];
}

export function androidTcpMapping(response, port, serial) {
    if (typeof response !== 'string' || response.length > 65536 || !Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid owned Android mapping evidence.');
    const lines = response.trim() ? response.trim().split(/\r?\n/) : [];
    if (lines.length > 128) throw new Error('Android mapping inventory exceeded its bound.');
    const matches = [];
    for (const line of lines) {
        const fields = line.trim().split(/\s+/);
        if (fields.length !== 3 || !/^[!-~]{1,128}$/.test(fields[0]) || fields.slice(1).some((value) => !/^tcp:[1-9][0-9]{0,4}$/.test(value) || Number(value.slice(4)) > 65535)) throw new Error('Unexpected Android mapping table.');
        if (fields[1] === `tcp:${port}`) matches.push(fields);
    }
    if (matches.length > 1 || matches.some((fields) => fields[2] !== `tcp:${port}` || serial && fields[0] !== serial)) throw new Error('Owned Android mapping changed identity.');
    return matches.length === 1;
}

export async function androidToolInputs(workspace) {
    const files = [];
    for (const name of ['android-app.mjs', 'android-stage.mjs', 'android-boot.mjs', 'android-evidence.mjs', 'android-owner.mjs',
        'android-runtime.mjs', 'android-fault-stage.mjs', 'android-fault-gateway.mjs', 'android-fault-policy.mjs', 'android-sdk.mjs', 'presets.mjs', 'qualify-vod-output.mjs', 'owned-process.mjs']) {
        files.push({ path: name, sha256: await hashFile(join(workspace, 'scripts/testing-memory', name)) });
    }
    files.push({ path: 'android-worker-media.mjs', sha256: await hashFile(join(workspace, 'scripts/qualification/android-worker-media.mjs')) });
    files.push({ path: 'commands.mjs', sha256: await hashFile(join(workspace, 'scripts/qualification/commands.mjs')) });
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
