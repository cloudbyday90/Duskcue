// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import net from 'node:net';
import { BrowserOwner } from './browser-owner.mjs';
import { FETCH_CASES, withRedirectSink } from './self-fetch.mjs';
import { validateMatrix } from './evidence.mjs';
import { assertSentinelPaths } from './preview.mjs';
import { LoopbackRequests } from './http.mjs';
import { ProcessOwners } from './process.mjs';

const tick = () => new Promise((resolve) => setImmediate(resolve));
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };

function ledger(platform = 'win32') {
    return [
        { id: 'reroute', status: 'passed', malformedPaths: 4, healthyRequests: 7 },
        { id: 'static-patterns', status: 'passed', literalPatterns: 21 },
        { id: 'cache-attribute', status: 'passed', artifactChecked: true },
        { id: 'cache-browser', status: 'passed', inlineControl: 1, markerAbsent: true, dialogs: 0, cleanupConfirmed: true },
        { id: 'client-origin', status: 'passed', hydrated: true, healthyRouteRequests: 1, hostileRouteRequests: 1, foreignRequests: 0, markerAbsent: true, cleanupConfirmed: true },
        { id: 'preview-windows', status: platform === 'win32' ? 'passed' : 'not-qualified', platform, positivePages: 1, positiveData: 1, sentinelVerified: true, blockedPaths: 2 },
        ...FETCH_CASES.map((item) => ({ id: item.id, status: 'passed', sinkCleanupConfirmed: true, httpStatus: item.external ? 200 : 302,
            location: item.external ? null : '/target', body: item.external ? 'target-nonce' : 'jump-nonce', requests: item.external ? [item.requested, '/target'] : [item.requested] })),
    ];
}

test('complete synthetic evidence never becomes full application security qualification', () => {
    assert.equal(validateMatrix(ledger(), 'nonce', 'win32').boundaryMatrixPassed, true);
    assert.equal(validateMatrix(ledger(), 'nonce', 'win32').completeSecurityMatrix, false);
    assert.equal(validateMatrix(ledger('linux'), 'nonce', 'linux').boundaryMatrixPassed, false);
    assert.throws(() => validateMatrix(ledger('linux'), 'nonce', 'win32'));
});

test('missing, duplicate, unknown and skipped case identities are refused', () => {
    for (const change of [(rows) => rows.pop(), (rows) => { rows[1].id = rows[0].id; }, (rows) => { rows[1].id = 'other'; }, (rows) => { rows[1].status = 'skipped'; }]) {
        const rows = ledger(); change(rows); assert.throws(() => validateMatrix(rows, 'nonce', 'win32'));
    }
});

test('positive controls, exact fetch sequences, nonce and cleanup are mandatory', () => {
    for (const [id, key, value] of [['cache-browser', 'inlineControl', 0], ['cache-browser', 'dialogs', 1], ['client-origin', 'healthyRouteRequests', 0],
        ['client-origin', 'hostileRouteRequests', 0], ['client-origin', 'foreignRequests', 1], ['client-origin', 'cleanupConfirmed', false],
        ['preview-windows', 'sentinelVerified', false], ['preview-windows', 'positiveData', 0], ['fetch-asset', 'requests', ['/asset.txt', '/target']],
        ['fetch-error', 'body', 'jump-stale'], ['fetch-external', 'httpStatus', 302], ['fetch-rerouted', 'sinkCleanupConfirmed', false]]) {
        const rows = ledger(); rows.find((row) => row.id === id)[key] = value;
        assert.throws(() => validateMatrix(rows, 'nonce', 'win32'), `${id}.${key}`);
    }
});

test('both raw Windows traversal probes reach the real parent sentinel on vulnerable path joining', () => { assertSentinelPaths(); });

function fakeBrowser(changes = {}) {
    const child = new EventEmitter();
    Object.assign(child, { pid: 12345, exitCode: null, signalCode: null, stdout: new PassThrough(), stderr: new PassThrough(), stdio: [] });
    const events = [];
    const context = { close: async () => { events.push('context-close'); } };
    const browser = { newContext: async () => context, close: async () => { events.push('connection-close'); } };
    const closed = () => { child.exitCode = 0; child.emit('close', 0, null); };
    const server = { process: () => child, wsEndpoint: () => 'ws://127.0.0.1:12345/token',
        close: async () => { events.push('server-close'); closed(); }, kill: async () => { events.push('server-kill'); closed(); } };
    const sdk = { launchServer: async () => server, connect: async () => browser, ...changes };
    return { sdk, child, events, server, browser, context, closed };
}

test('browser closes context, connection and native child in order, once', async () => {
    const fake = fakeBrowser();
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal, timeoutMs: 30 });
    assert.equal(await owner.open(), fake.context);
    await owner.close(); await owner.close();
    assert.deepEqual(fake.events, ['context-close', 'connection-close', 'server-close']);
    assert.equal(owner.cleanupConfirmed, true);
    await assert.rejects(owner.open(), /closed or occupied/);
});

test('cancelled late browser launch retains ownership and closes the actual late resource', async () => {
    const held = deferred(); const controller = new AbortController(); const fake = fakeBrowser({ launchServer: () => held.promise });
    const owner = new BrowserOwner(fake.sdk, { signal: controller.signal, timeoutMs: 15 });
    const opening = assert.rejects(owner.open(), /cancel launch/);
    await tick(); controller.abort(new Error('cancel launch')); await opening;
    await assert.rejects(owner.close(), /unconfirmed/); assert.equal(owner.cleanupConfirmed, false);
    held.resolve(fake.server); await owner.close();
    assert.deepEqual(fake.events, ['server-close']); assert.equal(owner.cleanupConfirmed, true);
});

test('browser disconnect or process exit does not replace observed child close', async () => {
    const fake = fakeBrowser();
    fake.server.close = async () => { fake.events.push('server-close'); fake.child.exitCode = 0; fake.child.emit('exit', 0); };
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal, timeoutMs: 15 });
    await owner.open();
    await assert.rejects(owner.close(), /unconfirmed/); assert.equal(owner.cleanupConfirmed, false);
    fake.closed(); await owner.close(); assert.equal(owner.cleanupConfirmed, true);
});

test('failed browser connection still joins its launched server', async () => {
    const fake = fakeBrowser({ connect: async () => { throw new Error('connection failed'); } });
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal });
    await assert.rejects(owner.open(), /connection failed/);
    await owner.close(); assert.deepEqual(fake.events, ['server-close']);
});

test('browser release failure remains failed even when fallback kill joins the child', async () => {
    const fake = fakeBrowser(); fake.server.close = async () => { throw new Error('close failed'); };
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal });
    await owner.open(); await assert.rejects(owner.close(), AggregateError);
    assert.equal(owner.cleanupConfirmed, false); assert.deepEqual(fake.events, ['context-close', 'connection-close', 'server-kill']);
    await assert.rejects(owner.close(), AggregateError);
});

test('late context creation after cancellation is retained and closed', async () => {
    const fake = fakeBrowser(); const held = deferred(); const controller = new AbortController();
    fake.browser.newContext = () => held.promise;
    const owner = new BrowserOwner(fake.sdk, { signal: controller.signal, timeoutMs: 15 });
    const opening = assert.rejects(owner.open(), /cancel context/);
    await tick(); controller.abort(new Error('cancel context')); await opening;
    assert.equal(owner.cleanupConfirmed, false);
    held.resolve(fake.context); await owner.close();
    assert.deepEqual(fake.events, ['context-close', 'connection-close', 'server-close']);
});

test('browser launches the exact admitted executable without inherited credentials', async () => {
    const fake = fakeBrowser(); let options;
    fake.sdk.launchServer = async (received) => { options = received; return fake.server; };
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal, executablePath: 'C:/owned/chrome.exe' });
    await owner.open(); await owner.close();
    assert.equal(options.executablePath, 'C:/owned/chrome.exe'); assert.equal(options.headless, true);
    assert.equal(options.host, '127.0.0.1'); assert.equal(options.env.GITHUB_TOKEN, undefined);
    assert.equal(options.env.GH_TOKEN, undefined);
});

for (const stage of ['context', 'server']) test(`stalled ${stage} release triggers native termination but retains late public completion`, async () => {
    const fake = fakeBrowser(); const held = deferred(); const killed = deferred();
    const target = stage === 'context' ? fake.context : fake.server;
    target.close = () => held.promise;
    fake.server.kill = async () => { fake.events.push('server-kill'); fake.closed(); killed.resolve(); };
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal, timeoutMs: 15 });
    await owner.open(); await assert.rejects(owner.close(), /unconfirmed/);
    await killed.promise; assert.equal(owner.cleanupConfirmed, false);
    await assert.rejects(owner.close(), /unconfirmed/);
    held.resolve(); await assert.rejects(owner.close(), AggregateError);
    assert.equal(fake.events.filter((item) => item === 'server-kill').length, 1);
    assert.equal(owner.cleanupConfirmed, false);
});

test('browser and Node share one finite child-output budget', async () => {
    const owners = new ProcessOwners({ outputBytes: 10 }); const fake = fakeBrowser(); let failures = 0;
    const owner = new BrowserOwner(fake.sdk, { signal: new AbortController().signal, output: (chunk) => owners.acceptOutput(chunk), fail: () => { failures++; } });
    owners.acceptOutput(Buffer.alloc(6)); await owner.open(); fake.child.stdout.emit('data', Buffer.alloc(5));
    for (let i = 0; i < 100; i++) fake.child.stdout.emit('data', Buffer.alloc(1));
    await assert.rejects(owner.close(), AggregateError);
    assert.equal(failures, 1); assert.equal(owners.bytes, 11); assert.equal(owner.cleanupConfirmed, false);
});

test('redirect sink returns exact loopback evidence and closes its listener on success', async () => {
    const requests = new LoopbackRequests(); let origin;
    try {
        const response = await withRedirectSink('nonce', new AbortController().signal, async (sink) => {
            origin = sink.origin; const result = await requests.get(origin, '/asset.txt');
            assert.deepEqual(sink.requests, ['/asset.txt']); return result;
        });
        assert.equal(response.status, 302); assert.equal(response.body, 'jump-nonce');
        await assert.rejects(requests.get(origin, '/target'), /ECONNREFUSED/);
    } finally { await requests.close(); }
});

test('redirect sink closes retained sockets and listener before reporting cancellation', async () => {
    const controller = new AbortController(); let socket; let origin;
    const work = withRedirectSink('nonce', controller.signal, async (sink) => {
        origin = sink.origin;
        socket = net.connect({ host: '127.0.0.1', port: Number(new URL(origin).port) });
        await new Promise((resolve, reject) => { socket.once('connect', resolve); socket.once('error', reject); });
        socket.write('GET /asset.txt HTTP/1.1\r\nHost: 127.0.0.1\r\n');
        controller.abort(new Error('cancel sink')); return {};
    });
    await assert.rejects(work, /cancel sink/);
    await new Promise((resolve) => { if (socket.destroyed) resolve(); else socket.once('close', resolve); });
    const requests = new LoopbackRequests();
    try { await assert.rejects(requests.get(origin, '/target'), /ECONNREFUSED/); } finally { await requests.close(); socket.destroy(); }
});
