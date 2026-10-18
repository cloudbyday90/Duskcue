// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, rename, writeFile } from 'node:fs/promises';

export function validateFormatReport(report, resourceId, enabled, completed) {
    const phase = report?.phase;
    if (!enabled || report?.resourceId !== resourceId || !/^format-(?:browser|native)-(?:0[0-9]|1[0-3])$/.test(phase || '')
        || completed.has(phase) || Buffer.byteLength(JSON.stringify(report)) > 65536) throw new Error('Invalid format verification.');
    return phase;
}

export function createFormatControl(resourceId, { enabled = false } = {}) {
    const completed = new Set();
    return async (report) => {
        const phase = validateFormatReport(report, resourceId, enabled, completed);
        completed.add(phase);
        const observed = `/tmp/direct-observed-${phase}`;
        await writeFile(`${observed}.pending`, JSON.stringify(report), { flag: 'wx', mode: 0o600 });
        await rename(`${observed}.pending`, `${observed}.json`);
        const deadline = Date.now() + 30000;
        while (Date.now() < deadline) {
            try {
                const bytes = await readFile(`/tmp/direct-verified-${phase}.json`);
                if (bytes.length > 65536) throw new Error('Format verification response exceeded its bound.');
                const verified = JSON.parse(bytes);
                if (verified.resourceId !== resourceId || verified.phase !== phase) throw new Error('Format verification identity changed.');
                return { verified };
            } catch (error) { if (error.code !== 'ENOENT') throw error; }
            await new Promise((resolve) => setTimeout(resolve, 100));
        }
        throw new Error('Format verification exceeded its deadline.');
    };
}
