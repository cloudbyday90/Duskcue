// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { spawn, execFile } from 'node:child_process';
import { writeFile, readFile } from 'node:fs/promises';
import { join, basename } from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { stopOwnedProcess } from '../../../../scripts/testing-memory/owned-process.mjs';
import { inspectNativeProcesses } from './qualification-processes.mjs';
import { repository } from './qualification-config.mjs';
import { assertOwnedNativeWindow, requireHostedWindowContext } from './qualification-window-control-policy.mjs';

const script = fileURLToPath(new URL('./qualification-window-control.ps1', import.meta.url));
const execute = promisify(execFile);

async function bounded(promise, message, milliseconds = 15000) {
    let timer;
    try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), milliseconds); })]); }
    finally { clearTimeout(timer); }
}

export async function warmHostedWindowControl(runtime, manifest) {
    const context = requireHostedWindowContext(process.env, repository, manifest);
    const snapshot = await inspectNativeProcesses(runtime);
    const actual = snapshot.processes.find((row) => row.pid === runtime.child.pid);
    if (!snapshot.hostVerified || !actual) throw new Error('Window control requires the verified live native host.');
    const host = { pid: actual.pid, parentPid: actual.parentPid, name: basename(manifest.executable), path: manifest.executable, createdAt: actual.createdAt, sha256: manifest.executableSha256 };
    const identityPath = join(manifest.directory, 'native-window-identity.json');
    await writeFile(identityPath, `${JSON.stringify({ identifier: manifest.identifier, context, host }, null, 2)}\n`, 'utf8');
    const startedAt = Date.now();
    const child = spawn('pwsh.exe', ['-NoProfile', '-File', script, '-IdentityPath', identityPath], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    child.startedAt = startedAt;
    const pending = new Map();
    let bytes = 0;
    let nextId = 1;
    let closed = false;
    let shutdown = false;
    let stderr = '';
    let rejectReady;
    let resolveReady;
    const ready = new Promise((resolve, reject) => { resolveReady = resolve; rejectReady = reject; });
    ready.catch(() => {});
    const exit = new Promise((resolve, reject) => { child.once('error', reject); child.once('close', (code, signal) => { closed = true; resolve({ code, signal }); }); });
    exit.catch(() => {});
    const lines = createInterface({ input: child.stdout });
    lines.on('line', (line) => {
        try {
            bytes += Buffer.byteLength(line);
            if (bytes > 131072 || line.length > 16384) throw new Error('Window helper exceeded its metadata bound.');
            const reply = JSON.parse(line);
            if (reply.id === 0) { if (reply.error) rejectReady(new Error(reply.error)); else resolveReady(reply); }
            else { const waiter = pending.get(reply.id); if (waiter) { pending.delete(reply.id); if (reply.error) waiter.reject(new Error(reply.error)); else waiter.resolve(reply); } }
        } catch (error) { rejectReady(error); for (const waiter of pending.values()) waiter.reject(error); pending.clear(); }
    });
    child.stderr.setEncoding('utf8');
    child.stderr.on('data', (chunk) => { stderr = (stderr + chunk).slice(-8192); });
    child.on('error', rejectReady);
    child.on('close', () => {
        const error = new Error(`Owned window helper closed before its pending response: ${stderr.slice(-1000)}`);
        rejectReady(error);
        for (const waiter of pending.values()) waiter.reject(error);
        pending.clear();
    });
    async function fallbackRestore() {
        if (!host.handle) return;
        await execute('pwsh.exe', ['-NoProfile', '-File', script, '-IdentityPath', identityPath, '-RestoreOnly'], { windowsHide: true, timeout: 15000, maxBuffer: 65536 });
        const proof = JSON.parse(await readFile(join(manifest.directory, 'native-window-fallback-restore.json'), 'utf8'));
        if (!proof.finallyRestored || proof.restoreError) throw new Error('The owned native fallback restoration failed.');
    }
    async function close() {
        if (shutdown) return;
        shutdown = true;
        let failure;
        try {
            if (!closed) child.stdin.end(`${JSON.stringify({ id: nextId++, action: 'close' })}\n`);
            const outcome = await bounded(exit, 'Owned window helper did not close after restoration.');
            if (outcome.code !== 0 || outcome.signal) throw new Error('The owned window helper failed before completing restoration.');
            const proof = JSON.parse(await readFile(join(manifest.directory, 'native-window-actions.json'), 'utf8'));
            if (!proof.finallyRestored || proof.restoreError) throw new Error('The owned native window was not restored.');
        } catch (error) {
            failure = error;
            if (!closed) { await stopOwnedProcess(child); await bounded(exit, 'Owned window helper pipes did not close.', 5000); }
            await fallbackRestore();
        } finally { lines.close(); child.stdout.resume(); child.stderr.resume(); }
        if (failure) throw failure;
    }
    try {
        const initial = await bounded(ready, 'Owned window helper did not establish its identity.');
        host.handle = assertOwnedNativeWindow(host, initial.host, initial.handles);
        await writeFile(identityPath, `${JSON.stringify({ identifier: manifest.identifier, context, host }, null, 2)}\n`, 'utf8');
        return {
            identity: host,
            async action(action) {
                if (!['minimize', 'restore'].includes(action) || closed || shutdown) throw new Error('The owned window action is unavailable.');
                const id = nextId++;
                const reply = new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
                reply.catch(() => {});
                child.stdin.write(`${JSON.stringify({ id, action })}\n`);
                const outcome = await bounded(reply, `Owned window ${action} response exceeded its deadline.`);
                assertOwnedNativeWindow(host, outcome.host, outcome.handles);
                if (outcome.iconic !== (action === 'minimize')) throw new Error('The actual native minimized state did not match the action.');
                return outcome;
            },
            close,
        };
    } catch (error) { await close().catch((cleanup) => { error.message += ` Window helper cleanup: ${cleanup.message}`; }); throw error; }
}
