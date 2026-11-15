// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawn } from 'node:child_process';
import { stopOwnedProcess } from '../../testing-memory/owned-process.mjs';
import { observe } from './lifecycle.mjs';

async function terminate(child) {
    if (process.platform === 'win32') return stopOwnedProcess(child);
    if (child.exitCode !== null || child.signalCode !== null) return;
    try { process.kill(-child.pid, 'SIGTERM'); }
    catch (error) { if (error.code !== 'ESRCH') throw error; }
    const force = setTimeout(() => {
        if (child.exitCode !== null || child.signalCode !== null) return;
        try { process.kill(-child.pid, 'SIGKILL'); } catch (error) { if (error.code !== 'ESRCH') child.emit('error', error); }
    }, 1000);
    child.once('close', () => clearTimeout(force));
}

export function fixtureEnvironment(extra = {}) {
    const selected = {};
    for (const name of ['PATH', 'Path', 'SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'SystemDrive', 'NODE_OPTIONS', 'UV_THREADPOOL_SIZE']) {
        if (process.env[name] !== undefined) selected[name] = process.env[name];
    }
    return { ...selected, CI: '1', ...extra };
}

export class ProcessOwners {
    #entries = new Set();
    #closing = false;
    #bytes = 0;
    #failure;
    #tail = Buffer.alloc(0);
    #signal;
    #terminate;
    #spawn;
    #stopTimeout;
    #outputLimit;
    constructor({ signal, stop = terminate, start = spawn, stopTimeoutMs = 10000, outputBytes = 16777216 } = {}) {
        this.#signal = signal;
        this.#terminate = stop;
        this.#spawn = start;
        this.#stopTimeout = stopTimeoutMs;
        this.#outputLimit = outputBytes;
    }
    get pending() { return this.#entries.size; }
    get bytes() { return this.#bytes; }
    get tail() { return this.#tail.toString('utf8'); }
    assertHealthy() { if (this.#failure) throw this.#failure; }

    start(args, { cwd, env = fixtureEnvironment(), nonce } = {}) {
        if (this.#closing) throw new Error('Process admission is closed.');
        if (this.#entries.size) throw new Error('An owned process is still pending.');
        this.#signal?.throwIfAborted();
        this.assertHealthy();
        const startedAt = Date.now();
        const child = this.#spawn(process.execPath, args, { cwd, env, windowsHide: true, detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe', 'ipc'] });
        child.startedAt = startedAt;
        let done;
        let ready;
        let notReady;
        let error;
        let actualStop;
        let reported = false;
        const closed = new Promise((resolve) => { done = resolve; });
        const readiness = new Promise((resolve, reject) => { ready = resolve; notReady = reject; });
        readiness.catch(() => {});
        const entry = {
            child, closed,
            stop: () => {
                actualStop ||= Promise.resolve().then(async () => { await this.#terminate(child); await closed; });
                return observe(actualStop, this.#stopTimeout, 'Owned process cleanup is unconfirmed; fixture retained.');
            },
            ready: (timeoutMs = 15000) => observe(readiness, timeoutMs, 'Owned server readiness deadline exceeded.'),
            wait: async (timeoutMs = 180000) => {
                const outcome = await observe(closed, timeoutMs, 'Owned command deadline exceeded.');
                if (error) throw error;
                this.assertHealthy();
                if (outcome.code !== 0) throw new Error(`Owned command failed (${outcome.code ?? outcome.signal}).`);
            },
            assertRunning: () => {
                this.assertHealthy();
                if (error || child.exitCode !== null || child.signalCode !== null) throw error || new Error('Owned server exited unexpectedly.');
            },
        };
        this.#entries.add(entry);
        const stopAfterFailure = (failure) => {
            if (this.#failure) return;
            this.#failure = failure;
            notReady(failure);
            entry.stop().catch((stopError) => { this.#failure ||= stopError; });
        };
        const cancelled = () => stopAfterFailure(this.#signal.reason || new Error('Qualification cancelled.'));
        for (const stream of [child.stdout, child.stderr]) stream.on('data', (chunk) => {
            if (this.#failure) return;
            this.#bytes += chunk.length;
            this.#tail = Buffer.concat([this.#tail, chunk]).subarray(-65536);
            if (this.#bytes > this.#outputLimit) stopAfterFailure(new Error('Shared child output budget exceeded.'));
        });
        child.on('message', (message) => {
            try {
                if (reported || !nonce || JSON.stringify(message).length > 1024 || message?.nonce !== nonce || message.pid !== child.pid || message.kind !== 'ready') throw new Error('Unbound or duplicate server readiness.');
                const origin = new URL(message.origin);
                if (origin.protocol !== 'http:' || origin.hostname !== '127.0.0.1' || !origin.port || origin.origin !== message.origin) throw new Error('Invalid owned server origin.');
                reported = true;
                ready(message.origin);
            } catch (failure) { stopAfterFailure(failure); }
        });
        child.once('error', (failure) => { error = failure; notReady(failure); });
        child.once('close', (code, signal) => {
            this.#entries.delete(entry);
            this.#signal?.removeEventListener('abort', cancelled);
            if (!reported) notReady(error || new Error('Owned command closed before readiness.'));
            done({ code, signal });
        });
        this.#signal?.addEventListener('abort', cancelled, { once: true });
        if (this.#signal?.aborted) cancelled();
        return entry;
    }

    async close() {
        this.#closing = true;
        const outcomes = await Promise.allSettled([...this.#entries].map((entry) => entry.stop()));
        const failures = outcomes.filter((item) => item.status === 'rejected').map((item) => item.reason);
        if (failures.length) throw new AggregateError(failures, 'Owned process cleanup failed; fixture retained.');
    }
}
