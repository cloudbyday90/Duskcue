// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';

test('actual bridge can cancel SSE with all eight data operations occupied', { timeout: 15000 }, async () => {
    const upstreams = [];
    const admitted = Promise.withResolvers();
    const server = createServer((request, response) => {
        upstreams.push({ path: request.url, response });
        if (request.url === '/api/v1/events') { response.writeHead(200, { 'content-type': 'text/event-stream' }); response.write('event: ready\ndata: {}\n\n'); }
        if (upstreams.length === 8) admitted.resolve();
    });
    let child;
    let lines;
    let closed;
    const replies = new Map();
    const waiting = new Map();
    const observed = (id) => {
        if (replies.has(id)) return Promise.resolve(replies.get(id));
        const pending = Promise.withResolvers();
        waiting.set(id, pending);
        return pending.promise;
    };
    const bounded = async (promise) => {
        let timer;
        try { return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('Bridge cancellation did not settle.')), 5000); })]); }
        finally { clearTimeout(timer); }
    };
    try {
        await new Promise((resolve, reject) => { server.once('error', reject); server.listen(38081, '127.0.0.1', resolve); });
        child = spawn(process.execPath, [fileURLToPath(new URL('../../docker/qualification/direct-http-bridge.mjs', import.meta.url))], {
            windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, DUSKCUE_TEST_RESOURCE_ID: 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee', DUSKCUE_TEST_DIRECT_BROWSER: 'live' },
        });
        closed = new Promise((resolve) => child.once('close', resolve));
        child.once('error', admitted.reject);
        child.stdin.on('error', () => {});
        child.stderr.resume();
        lines = createInterface({ input: child.stdout });
        lines.on('line', (line) => {
            const message = JSON.parse(line);
            if (message.type === 'headers' || message.type === 'chunk') return;
            replies.set(message.id, message);
            waiting.get(message.id)?.resolve(message);
        });
        child.stdin.write(`${JSON.stringify({ id: 1, operation: 'stream', method: 'GET', path: '/api/v1/events', headers: {}, body: '' })}\n`);
        for (let id = 2; id <= 8; id++) child.stdin.write(`${JSON.stringify({ id, operation: 'fetch', method: 'GET', path: '/api/v1/profiles?limit=1', headers: {}, body: '' })}\n`);
        await bounded(admitted.promise);
        child.stdin.write(`${JSON.stringify({ id: 9, operation: 'fetch', method: 'GET', path: '/api/v1/profiles?limit=1', headers: {}, body: '' })}\n`);
        assert.match((await bounded(observed(9))).error, /bound exceeded/);
        assert.equal(upstreams.length, 8);
        child.stdin.write(`${JSON.stringify({ id: 10, operation: 'cancel', target: 1 })}\n`);
        assert.equal((await bounded(observed(10))).cancelled, true);
        assert.equal((await bounded(observed(1))).type, 'end');
        for (const entry of upstreams) if (entry.path !== '/api/v1/events') entry.response.end('{}');
        for (let id = 2; id <= 8; id++) assert.equal((await bounded(observed(id))).status, 200);
        assert.equal(replies.has(0), false);
    } finally {
        lines?.close();
        child?.kill();
        if (closed) await bounded(closed);
        server.closeAllConnections();
        await new Promise((resolve) => server.close(resolve));
    }
});
