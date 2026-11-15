// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import http from 'node:http';

export class LoopbackRequests {
    #pending = new Set();
    #closing = false;
    get pending() { return this.#pending.size; }

    get(origin, path, { signal, timeoutMs = 5000, maxBytes = 65536 } = {}) {
        if (this.#closing) throw new Error('HTTP admission is closed.');
        if (this.#pending.size) throw new Error('An owned HTTP operation is still pending.');
        if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 5000 || !Number.isInteger(maxBytes) || maxBytes < 1 || maxBytes > 65536) throw new Error('Invalid HTTP operation bound.');
        signal?.throwIfAborted();
        const url = new URL(origin);
        if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port || url.origin !== origin || !path.startsWith('/') || /[\r\n]/.test(path)) {
            throw new Error('Only an exact loopback fixture origin and path are allowed.');
        }
        let done;
        const closed = new Promise((resolve) => { done = resolve; });
        let request;
        let socket;
        let requestClosed = false;
        let socketClosed = false;
        let value;
        let failure;
        const marker = { closed, cancel: () => request?.destroy(new Error('Owned request cancelled.')) };
        this.#pending.add(marker);
        const finish = () => {
            if (!requestClosed || socket && !socketClosed) return;
            clearTimeout(timer);
            signal?.removeEventListener('abort', abort);
            this.#pending.delete(marker);
            done();
        };
        const abort = () => marker.cancel();
        const timer = setTimeout(() => request?.destroy(new Error('Owned request deadline exceeded.')), timeoutMs);
        try {
            request = http.request({ hostname: '127.0.0.1', port: Number(url.port), method: 'GET', path, agent: false, maxHeaderSize: 16384,
                headers: { connection: 'close', host: url.host } });
            request.on('socket', (owned) => {
                socket = owned;
                owned.once('close', () => { socketClosed = true; finish(); });
            });
            request.once('error', (error) => { failure ||= error; });
            request.once('response', (response) => {
                const chunks = [];
                let bytes = 0;
                response.on('data', (chunk) => {
                    bytes += chunk.length;
                    if (bytes > maxBytes) { failure ||= new Error('Owned response exceeded its body bound.'); response.destroy(failure); request.destroy(failure); }
                    else chunks.push(chunk);
                });
                response.once('error', (error) => { failure ||= error; request.destroy(error); });
                response.once('end', () => {
                    if (!response.complete) failure ||= new Error('Owned response was truncated.');
                    value = { status: response.statusCode, headers: response.headers, body: Buffer.concat(chunks).toString('utf8') };
                });
                response.once('close', () => { if (!response.complete) { failure ||= new Error('Owned response was truncated.'); request.destroy(failure); } });
            });
            request.once('close', () => { requestClosed = true; finish(); });
            signal?.addEventListener('abort', abort, { once: true });
            if (signal?.aborted) abort();
            request.end();
        } catch (error) {
            failure ||= error;
            if (request) request.destroy(error);
            else { requestClosed = true; finish(); }
        }
        return closed.then(() => { if (failure) throw failure; if (!value) throw new Error('Owned response did not finish.'); return value; });
    }

    async close() {
        this.#closing = true;
        const pending = [...this.#pending];
        pending.forEach((entry) => entry.cancel());
        await Promise.all(pending.map((entry) => entry.closed));
    }
}
