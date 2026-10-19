// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { waitForFormatPacing } from './qualification-format-pacing.mjs';

export function assertSourceRecoveryStartup(records, generation) {
    if (records.some((record) => record.status === 429)) throw new Error('Source recovery startup was rate limited by the production router.');
    const counts = new Map();
    for (const record of records) {
        if (record.method !== 'GET' || record.status !== 200 || !record.path.includes(`/transcode/${generation}/`) || !record.path.endsWith('.m4s')) continue;
        const count = (counts.get(record.path) || 0) + 1;
        counts.set(record.path, count);
        if (count > 8) throw new Error(`Source recovery startup repeatedly loaded ${record.path.split('/').at(-1)} before usable media (${count} successful responses).`);
    }
}

export async function waitForSourceRecoveryStartup({ readMedia, readRecords, generation, signal, previousSource = null,
    now = () => performance.now(), wait = waitForFormatPacing }) {
    const deadline = now() + 5000;
    for (let attempt = 0; attempt <= 50; attempt += 1) {
        if (signal?.aborted) throw new Error('Actual source decoder recovery startup was canceled.');
        assertSourceRecoveryStartup(readRecords(), generation);
        const media = await readMedia();
        if (media.ready >= 2 && media.source?.startsWith('blob:') && (!previousSource || media.source !== previousSource)) return media;
        const remaining = deadline - now();
        if (remaining <= 0) break;
        await wait(Math.min(100, Math.ceil(remaining)), signal);
    }
    throw new Error('Source recovery startup did not produce usable media within five seconds.');
}
