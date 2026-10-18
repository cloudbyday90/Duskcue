// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';

export function openDirectBridge(context, runtime, abortSignal) {
    if (!/^[A-Za-z0-9_.-]+$/.test(context) || !/^[a-f0-9]{64}$/.test(runtime)) throw new Error('Owned bridge identity is invalid.');
    const child = spawn('docker.exe', ['--context', context, 'exec', '-i', runtime, 'node', '/qualification/direct-http-bridge.mjs'], { windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    const pending = new Map();
    let sequence = 0;
    let stderrBytes = 0;
    let closed = false;
    const fail = () => { for (const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(new Error('Owned HTTP bridge terminated.')); } pending.clear(); };
    const exit = new Promise((resolve) => child.once('close', (code) => { closed = true; fail(); resolve(code); }));
    child.on('error', fail);
    child.stdin.on('error', fail);
    child.stderr.on('data', (chunk) => { stderrBytes += chunk.length; if (stderrBytes > 16384) child.kill(); });
    const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
    lines.on('line', (line) => {
        if (line.length > 12582912) { child.kill(); fail(); return; }
        try {
            const response = JSON.parse(line);
            const entry = pending.get(response.id);
            if (!entry) throw new Error('Unknown bridge response.');
            if (entry.onData && !response.error && response.type !== 'end') { entry.onData(response); return; }
            pending.delete(response.id); clearTimeout(entry.timer);
            if (response.error) entry.reject(new Error(response.error)); else entry.resolve(response);
        } catch { child.kill(); fail(); }
    });
    const abort = () => child.kill();
    abortSignal?.addEventListener('abort', abort, { once: true });
    if (abortSignal?.aborted) abort();
    function send(message, onData) {
        if (closed || abortSignal?.aborted || pending.size >= 8) throw new Error('Owned HTTP bridge is unavailable.');
        const id = ++sequence;
        const done = new Promise((resolve, reject) => {
            const timer = setTimeout(() => { pending.delete(id); reject(new Error('Owned HTTP bridge exceeded its deadline.')); child.kill(); }, onData ? 125000 : 35000);
            pending.set(id, { resolve, reject, timer, onData });
            child.stdin.write(`${JSON.stringify({ ...message, id })}\n`);
        });
        return { id, done };
    }
    return {
        request(message) {
            try { return send(message).done; } catch (error) { return Promise.reject(error); }
        },
        stream(message, onData) {
            const { id, done } = send({ ...message, operation: 'stream' }, onData);
            return { done, cancel() { if (!pending.has(id)) return Promise.resolve(); return send({ operation: 'cancel', target: id }).done; } };
        },
        async close() {
            abortSignal?.removeEventListener('abort', abort);
            child.stdin.end();
            const awaitExit = async () => {
                let timer;
                try { return await Promise.race([exit.then(() => true), new Promise((resolve) => { timer = setTimeout(() => resolve(false), 5000); })]); }
                finally { clearTimeout(timer); }
            };
            try {
                if (!await awaitExit()) {
                    child.kill();
                    if (!await awaitExit()) throw new Error('Owned HTTP bridge exit was not confirmed.');
                }
            } finally { lines.close(); fail(); }
        },
    };
}
