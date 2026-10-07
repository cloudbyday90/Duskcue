/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { execFile } from 'node:child_process';
import { basename } from 'node:path';

export async function stopOwnedProcess(child) {
    if (!child?.pid || child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
    if (!Number.isFinite(child.startedAt)) throw new Error('The owned process creation time is unavailable; no process was terminated.');
    const actual = await new Promise((resolve, reject) => {
        const command = `$candidate = Get-CimInstance Win32_Process -Filter 'ProcessId = ${child.pid}'; if ($candidate) { [pscustomobject]@{parentPid=$candidate.ParentProcessId; name=$candidate.Name; createdAt=$candidate.CreationDate.ToUniversalTime().ToString('o')} | ConvertTo-Json -Compress }`;
        execFile('pwsh.exe', ['-NoProfile', '-Command', command], { windowsHide: true, timeout: 10000 }, (error, output) => {
            if (error) reject(new Error('The owned process could not be revalidated.'));
            else { try { resolve(output.trim() ? JSON.parse(output) : null); } catch { reject(new Error('The owned process identity was invalid.')); } }
        });
    });
    if (!actual || child.exitCode !== null || child.signalCode !== null) return;
    const createdAt = Date.parse(actual.createdAt);
    if (actual.parentPid !== process.pid || actual.name.toLowerCase() !== basename(child.spawnfile).toLowerCase()
        || !Number.isFinite(createdAt) || createdAt < child.startedAt - 2000 || createdAt > child.startedAt + 5000) {
        throw new Error('The process is no longer the owned testing command; no process was terminated.');
    }
    return new Promise((resolve, reject) => {
        execFile('taskkill.exe', ['/PID', String(child.pid), '/T', '/F'], { windowsHide: true, timeout: 10000 }, (error) => {
            if (error && child.exitCode === null && child.signalCode === null) reject(new Error('Could not stop the owned testing process tree.'));
            else resolve();
        });
    });
}

export function ownedProcessBytes(sample, rootPid) {
    const owned = new Set([rootPid]);
    let changed = true;
    while (changed) {
        changed = false;
        for (const row of sample.processes || []) {
            if (owned.has(row.parentPid) && !owned.has(row.pid)) { owned.add(row.pid); changed = true; }
        }
    }
    const rows = (sample.processes || []).filter((row) => owned.has(row.pid));
    return { processCount: rows.length, privateBytes: rows.reduce((sum, row) => sum + row.privateBytes, 0), workingSetBytes: rows.reduce((sum, row) => sum + row.workingSetBytes, 0) };
}
