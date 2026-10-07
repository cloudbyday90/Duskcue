/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { appendFile, realpath } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { stopOwnedProcess } from './owned-process.mjs';
import { cleanupOwnedContainers } from './owned-containers.mjs';

async function command(workspace, args) {
    const startedAt = Date.now();
    const child = spawn('docker.exe', args, { cwd: workspace, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
    child.startedAt = startedAt;
    let stdout = '';
    let stderr = '';
    let bytes = 0;
    let failure;
    let stopping;
    const stop = (message) => {
        failure ||= message;
        stopping ||= stopOwnedProcess(child);
        stopping.catch(() => {});
    };
    const result = new Promise((resolveResult, reject) => {
        child.once('error', reject);
        child.once('close', (code, signal) => resolveResult({ code, signal }));
    });
    result.catch(() => {});
    const timeout = setTimeout(() => stop('The owned Docker diagnostic exceeded its deadline.'), 120000);
    const interrupt = () => stop('The owned Docker diagnostic was interrupted.');
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', interrupt);
    for (const [stream, target] of [[child.stdout, 'stdout'], [child.stderr, 'stderr']]) {
        stream.setEncoding('utf8');
        stream.on('data', (data) => {
            bytes += Buffer.byteLength(data);
            if (bytes > 1048576) { stop('The owned Docker diagnostic exceeded its output limit.'); return; }
            if (target === 'stdout') stdout += data;
            else stderr += data;
        });
    }
    try {
        const outcome = await result;
        if (stopping) await stopping;
        if (failure) throw new Error(failure);
        return { ...outcome, stdout, stderr };
    } finally {
        clearTimeout(timeout);
        process.off('SIGINT', interrupt);
        process.off('SIGTERM', interrupt);
        if (child.pid && child.exitCode === null && child.signalCode === null) {
            await stopOwnedProcess(child);
            await result;
        }
    }
}

export async function createGuardedDocker(workspace) {
    const ownerDirectory = await realpath(join(workspace, '.cache', 'testing-memory'));
    const resourceId = process.env.DUSKCUE_TEST_RESOURCE_ID;
    const marker = process.env.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(resourceId || '') || !marker
        || resolve(marker) !== join(ownerDirectory, `${resourceId}.containers.jsonl`)) {
        throw new Error('The Docker diagnostic requires an owning memory workflow.');
    }
    return {
        resourceId,
        command: (args) => command(workspace, args),
        async container(image, executable, args, { directory, mounts = [], retain = false } = {}) {
            if (!/^sha256:[a-f0-9]{64}$/.test(image)) throw new Error('Diagnostic runtime images must be immutable SHA256 identities.');
            const name = `duskcue-playback-${randomUUID()}`;
            await appendFile(marker, `${JSON.stringify({ resourceId, name })}\n`, 'utf8');
            const launch = [
                'run', '--pull=never', '--name', name,
                '--label', `duskcue.test.resource-id=${resourceId}`,
                '--memory=512m', '--memory-swap=512m', '--cpus=2', '--pids-limit=128',
            ];
            if (!retain) launch.push('--rm');
            if (directory) launch.push('--mount', `type=bind,source=${directory},target=/fixtures`);
            for (const mount of mounts) launch.push('--mount', mount);
            launch.push('--entrypoint', executable, image, ...args);
            return { name, ...await command(workspace, launch) };
        },
        cleanup: () => cleanupOwnedContainers(marker, resourceId),
    };
}
