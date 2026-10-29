// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { hashFile } from '../qualification/source.mjs';

export function redactAndroidOutput(text) {
    return text.replace(/http:\/\/127\.0\.0\.1:\d+\/[A-Za-z0-9_-]{43}(?:\/[^\s'"<>)]*)?/g, '[redacted-relay-url]');
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

export async function androidToolInputs(workspace) {
    const files = [];
    for (const name of ['android-app.mjs', 'android-boot.mjs', 'android-evidence.mjs', 'android-owner.mjs',
        'android-runtime.mjs', 'android-sdk.mjs', 'presets.mjs', 'qualify-vod-output.mjs', 'owned-process.mjs']) {
        files.push({ path: name, sha256: await hashFile(join(workspace, 'scripts/testing-memory', name)) });
    }
    files.push({ path: 'commands.mjs', sha256: await hashFile(join(workspace, 'scripts/qualification/commands.mjs')) });
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
