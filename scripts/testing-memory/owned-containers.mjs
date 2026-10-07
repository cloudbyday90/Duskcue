// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { readFile } from 'node:fs/promises';
import { execFile } from 'node:child_process';

function docker(args, context) {
    return new Promise((resolve, reject) => {
        execFile('docker.exe', context ? ['--context', context, ...args] : args, { windowsHide: true, timeout: 10000, maxBuffer: 65536 }, (error, output) => {
            if (error) reject(new Error('The owned test containers could not be inspected or cleaned.'));
            else resolve(output.trim());
        });
    });
}

export function parseOwnedContainers(content, resourceId) {
    if (!/^[a-f0-9-]{36}$/.test(resourceId)) throw new Error('The test resource identity is invalid.');
    const candidates = new Map();
    for (const line of content.split('\n').filter(Boolean)) {
        const candidate = JSON.parse(line);
        if (candidate.resourceId !== resourceId || !/^duskcue-playback-[a-f0-9-]{36}$/.test(candidate.name)
            || candidate.context !== undefined && !/^[A-Za-z0-9_.-]+$/.test(candidate.context)) throw new Error('The container marker is outside this testing workflow.');
        if (candidates.has(candidate.name) && candidates.get(candidate.name) !== candidate.context) throw new Error('The owned container context changed in its marker.');
        candidates.set(candidate.name, candidate.context);
    }
    return candidates;
}

export async function cleanupOwnedContainers(markerPath, resourceId) {
    if (!markerPath) return [];
    let content;
    try { content = await readFile(markerPath, 'utf8'); }
    catch (error) { if (error.code === 'ENOENT') return []; throw error; }
    const candidates = parseOwnedContainers(content, resourceId);
    const removed = [];
    for (const [name, context] of candidates) {
        const ids = (await docker(['ps', '-aq', '--filter', `name=^${name}$`, '--filter', `label=duskcue.test.resource-id=${resourceId}`], context)).split(/\s+/).filter(Boolean);
        for (const id of ids) {
            if (!/^[a-f0-9]{12,64}$/.test(id)) throw new Error('The Docker container identity is invalid.');
            const labels = JSON.parse(await docker(['inspect', '--format', '{{json .Config.Labels}}', id], context));
            if (labels?.['duskcue.test.resource-id'] !== resourceId) throw new Error('The container no longer belongs to this testing workflow.');
            await docker(['rm', '--force', id], context);
            removed.push(id);
        }
    }
    return removed;
}
