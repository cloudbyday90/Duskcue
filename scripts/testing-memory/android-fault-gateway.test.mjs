// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, request } from 'node:http';
import { once } from 'node:events';
import { createFaultGateway } from './android-fault-gateway.mjs';
import { faultRoute, fixedOrigin } from './android-fault-policy.mjs';

const id = '019a35de-1234-7000-8000-123456789abc';
const root = `/api/v1/playback-commands/${id}`;
async function backend(handler) {
    const server = createServer(handler);
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    return { origin: `http://127.0.0.1:${server.address().port}`, close: () => new Promise((resolve, reject) => {
        server.close((error) => error ? reject(error) : resolve()); server.closeAllConnections();
    }) };
}
function call(origin, method, path, token = 'fixture-only', headers = {}) {
    return new Promise((resolve, reject) => {
        const outgoing = request(origin + path, { method, agent: false, headers: { authorization: `Bearer ${token}`, 'x-session-context': 'bound-context', ...headers } }, (incoming) => {
            const parts = [];
            incoming.on('data', (part) => parts.push(part));
            incoming.on('error', reject);
            incoming.on('end', () => resolve({ status: incoming.statusCode, body: Buffer.concat(parts).toString(), headers: incoming.headers }));
        });
        outgoing.on('error', reject);
        outgoing.end();
    });
}

test('real accepted responses are lost once, without a retry or credential-bearing trace', async () => {
    const seen = [];
    const upstream = await backend((req, res) => {
        seen.push(`${req.method} ${req.url}`);
        assert.equal(req.headers.authorization, 'Bearer fixture-only');
        assert.equal(req.headers['x-session-context'], 'bound-context');
        res.writeHead(req.method === 'POST' ? 202 : 200, { 'content-type': 'application/json' });
        res.end('{"real_upstream":true}');
    });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        gateway.arm();
        for (const path of ['/api/v1/playback-commands', `${root}/configuration`, `${root}/renew`, `${root}/withdraw`]) {
            await assert.rejects(call(gateway.origin, 'POST', path));
        }
        assert.equal((await call(gateway.origin, 'GET', `${root}/settlement`)).status, 200);
        gateway.verify();
        assert.equal(seen.length, 5);
        assert.ok(!JSON.stringify(gateway.trace).includes('fixture-only'));
        assert.ok(!JSON.stringify(gateway.trace).includes('bound-context'));
    } finally { await gateway.close(); await upstream.close(); }
    await assert.rejects(call(gateway.origin, 'GET', root));
});

test('fixed route and credential checks refuse traversal, queries, other origins and methods', async () => {
    let hits = 0;
    const upstream = await backend((req, res) => { hits++; res.end('{}'); });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        assert.equal((await call(gateway.origin, 'POST', root, 'other')).status, 401);
        for (const path of [`${root}/renew?redirect=yes`, `${root}/../renew`, 'http://example.org/api/v1/playback-commands', `${root}/outputs/${id}/hls/../../manifest.m3u8`]) {
            assert.throws(() => faultRoute('POST', path));
        }
        assert.equal((await call(gateway.origin, 'DELETE', root)).status, 404);
        assert.equal(hits, 0);
        for (const origin of ['https://127.0.0.1:1234', 'http://localhost:1234', 'http://127.0.0.1:1234/', 'http://user@127.0.0.1:1234']) assert.throws(() => fixedOrigin(origin));
    } finally { await gateway.close(); await upstream.close(); }
});

test('redirect is returned unchanged and never followed', async () => {
    let hits = 0;
    const canary = await backend((req, res) => { hits++; res.end('canary'); });
    const upstream = await backend((req, res) => { res.writeHead(302, { location: canary.origin }); res.end(); });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        gateway.arm();
        assert.equal((await call(gateway.origin, 'POST', '/api/v1/playback-commands')).status, 302);
        assert.equal(hits, 0);
        assert.equal(gateway.trace[0].dropped, false);
        assert.throws(() => gateway.verify());
    } finally { await gateway.close(); await upstream.close(); await canary.close(); }
});

test('oversized or incomplete response cannot become successful loss evidence', async () => {
    let attempt = 0;
    const upstream = await backend((req, res) => {
        if (++attempt === 1) res.end(Buffer.alloc(32769));
        else { res.writeHead(200, { 'content-length': 20 }); res.write('short'); setImmediate(() => res.destroy()); }
    });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        gateway.arm();
        await assert.rejects(call(gateway.origin, 'POST', '/api/v1/playback-commands'));
        await assert.rejects(call(gateway.origin, 'POST', '/api/v1/playback-commands'));
        assert.ok(gateway.trace.every((entry) => !entry.complete && !entry.dropped));
        assert.throws(() => gateway.verify());
    } finally { await gateway.close(); await upstream.close(); }
});

test('capacity refuses a ninth exchange while close observes all pending sockets', async () => {
    let hits = 0;
    let full;
    const ready = new Promise((resolve) => { full = resolve; });
    const upstream = await backend(() => { if (++hits === 8) full(); });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    const held = Array.from({ length: 8 }, () => call(gateway.origin, 'GET', root).then(() => false, () => true));
    try {
        await ready;
        assert.equal((await call(gateway.origin, 'GET', root)).status, 503);
        assert.equal(hits, 8);
        await gateway.close();
        assert.deepEqual(await Promise.all(held), Array(8).fill(true));
    } finally { await gateway.close(); await upstream.close(); }
});

test('native phase arms renewal loss without changing earlier regular renewal', async () => {
    const upstream = await backend((req, res) => res.end('{}'));
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        gateway.arm({ deferRenew: true });
        assert.equal((await call(gateway.origin, 'POST', `${root}/renew`)).status, 200);
        gateway.armRenew();
        assert.throws(() => gateway.armRenew());
        for (const path of ['/api/v1/playback-commands', `${root}/configuration`, `${root}/renew`, `${root}/withdraw`]) await assert.rejects(call(gateway.origin, 'POST', path));
        await call(gateway.origin, 'GET', `${root}/settlement`);
        gateway.verify();
    } finally { await gateway.close(); await upstream.close(); }
});

test('streamed media preserves exact range framing and forwards the original range', async () => {
    const upstream = await backend((req, res) => {
        assert.equal(req.headers.range, 'bytes=0-3');
        res.writeHead(206, { 'content-length': 4, 'content-range': 'bytes 0-3/8', 'content-type': 'video/mp4' });
        res.end('data');
    });
    const gateway = await createFaultGateway({ ...upstream, token: 'fixture-only' });
    try {
        const response = await call(gateway.origin, 'GET', `${root}/outputs/${id}/hls/v/init.mp4`, 'fixture-only', { range: 'bytes=0-3' });
        assert.equal(response.status, 206);
        assert.equal(response.body, 'data');
        assert.equal(response.headers['content-range'], 'bytes 0-3/8');
        assert.equal(response.headers['content-length'], '4');
    } finally { await gateway.close(); await upstream.close(); }
});
