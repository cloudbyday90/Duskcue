// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createInterface } from 'node:readline';

export function createDirectBridgeSession(child, { abortSignal } = {}) {
    const pending = new Map();
    let sequence = 0;
    let stderrBytes = 0;
    let closed = false;
    let failed = false;
    let closing;
    const fail = () => {
        failed = true;
        for (const entry of pending.values()) { clearTimeout(entry.timer); entry.reject(new Error('Owned HTTP bridge terminated.')); }
        pending.clear();
    };
    const kill = () => { try { child.kill(); } finally { fail(); } };
    const exit = new Promise((resolve) => child.once('close', (code) => { closed = true; fail(); resolve(code); }));
    child.on('error', fail);
    child.stdin.on('error', fail);
    child.stderr.on('data', (chunk) => { stderrBytes += chunk.length; if (stderrBytes > 16384) kill(); });
    const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
    lines.on('line', (line) => {
        if (line.length > 12582912) { kill(); return; }
        try {
            const response = JSON.parse(line);
            const entry = pending.get(response.id);
            if (!entry) throw new Error('Unknown bridge response.');
            if (entry.onData && !response.error && response.type !== 'end') { entry.onData(response); return; }
            pending.delete(response.id);
            clearTimeout(entry.timer);
            if (response.error) entry.reject(new Error(response.error)); else entry.resolve(response);
        } catch { kill(); }
    });
    const abort = () => kill();
    abortSignal?.addEventListener('abort', abort, { once: true });
    if (abortSignal?.aborted) abort();

    function send(message, onData) {
        const kind = message.operation === 'cancel' ? 'control' : message.operation === 'stream' ? 'stream' : 'data';
        const entries = [...pending.values()];
        const atLimit = kind === 'control' ? entries.filter((entry) => entry.kind === 'control').length >= 2
            : entries.filter((entry) => entry.kind !== 'control').length >= 8;
        if (closed || failed || closing || abortSignal?.aborted || atLimit
            || kind === 'stream' && entries.filter((entry) => entry.kind === 'stream').length >= 2) throw new Error('Owned HTTP bridge is unavailable.');
        const id = ++sequence;
        const timeout = kind === 'control' ? 5000 : kind === 'stream' ? 125000 : 35000;
        const done = new Promise((resolve, reject) => {
            const timer = setTimeout(() => { pending.delete(id); reject(new Error('Owned HTTP bridge exceeded its deadline.')); kill(); }, timeout);
            pending.set(id, { resolve, reject, timer, onData, kind });
            try { child.stdin.write(`${JSON.stringify({ ...message, id })}\n`); }
            catch { kill(); }
        });
        return { id, done };
    }

    return {
        request(message) {
            try { return send(message).done; } catch (error) { return Promise.reject(error); }
        },
        stream(message, onData) {
            if (typeof onData !== 'function') throw new Error('Owned event stream observation is required.');
            const { id, done } = send({ ...message, operation: 'stream' }, onData);
            let canceled;
            return {
                done,
                cancel() {
                    canceled ||= Promise.resolve().then(() => {
                        if (closed || failed || closing || abortSignal?.aborted) throw new Error('Owned HTTP bridge is unavailable.');
                        if (!pending.has(id)) return { cancelled: true };
                        return send({ operation: 'cancel', target: id }).done;
                    });
                    return canceled;
                },
            };
        },
        close() {
            closing ||= (async () => {
                abortSignal?.removeEventListener('abort', abort);
                child.stdin.end();
                const awaitExit = async () => {
                    let timer;
                    try { return await Promise.race([exit.then(() => true), new Promise((resolve) => { timer = setTimeout(() => resolve(false), 5000); })]); }
                    finally { clearTimeout(timer); }
                };
                try {
                    if (!await awaitExit()) {
                        kill();
                        if (!await awaitExit()) throw new Error('Owned HTTP bridge exit was not confirmed.');
                    }
                } finally { lines.close(); fail(); }
            })();
            return closing;
        },
    };
}
