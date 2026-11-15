// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { once } from 'node:events';
import { LoopbackRequests } from './http.mjs';

async function withServer(handle, run) {
    const sockets = new Set();
    const server = http.createServer(handle);
    server.on('connection', (socket) => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); });
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    const origin = `http://127.0.0.1:${server.address().port}`;
    const requests = new LoopbackRequests();
    try { await run(requests, origin); }
    finally {
        await requests.close();
        const closed = new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
        for (const socket of sockets) socket.destroy();
        await closed;
    }
}

test('bounded literal-loopback HTTP preserves raw malformed paths and refuses redirects', async () => {
    const paths = [];
    await withServer((request, response) => {
        paths.push(request.url);
        if (request.url === '/redirect') { response.writeHead(302, { location: '/destination' }); response.end('redirect'); }
        else response.end('healthy');
    }, async (requests, origin) => {
        assert.equal((await requests.get(origin, '/bad/%FF')).body, 'healthy');
        assert.equal((await requests.get(origin, '/redirect')).status, 302);
        assert.deepEqual(paths, ['/bad/%FF', '/redirect']);
        assert.equal(requests.pending, 0);
        for (const candidate of ['https://127.0.0.1:1234', 'http://localhost:1234', `${origin}/extra`]) assert.throws(() => requests.get(candidate, '/'));
    });
});

test('response overflow destroys and joins owned HTTP before capacity returns', async () => {
    await withServer((_, response) => response.end(Buffer.alloc(8192)), async (requests, origin) => {
        await assert.rejects(requests.get(origin, '/large', { maxBytes: 1024 }), /body bound/);
        assert.equal(requests.pending, 0);
    });
});

test('truncated framing fails instead of accepting an incomplete body', async () => {
    await withServer((_, response) => { response.writeHead(200, { 'content-length': '100' }); response.end('short'); }, async (requests, origin) => {
        await assert.rejects(requests.get(origin, '/truncated'));
        assert.equal(requests.pending, 0);
    });
});

test('stall deadline cancels I/O and completion releases the one-operation slot', async () => {
    await withServer((_, response) => { response.writeHead(200); response.write('held'); }, async (requests, origin) => {
        const pending = requests.get(origin, '/stall', { timeoutMs: 50 });
        const rejected = assert.rejects(pending, /deadline/);
        assert.equal(requests.pending, 1);
        assert.throws(() => requests.get(origin, '/second'), /still pending/);
        await rejected;
        assert.equal(requests.pending, 0);
    });
});

test('scope cancellation and shutdown join request/socket closure and close admission', async () => {
    let entered;
    const seen = new Promise((resolve) => { entered = resolve; });
    await withServer((_, response) => { response.write('held'); entered(); }, async (requests, origin) => {
        const controller = new AbortController();
        const rejected = assert.rejects(requests.get(origin, '/stall', { signal: controller.signal }), /cancelled/);
        await seen;
        controller.abort();
        await requests.close();
        await rejected;
        assert.equal(requests.pending, 0);
        assert.throws(() => requests.get(origin, '/late'), /admission is closed/);
        await requests.close();
    });
});
