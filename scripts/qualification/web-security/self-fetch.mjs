// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import http from 'node:http';

export const FETCH_CASES = Object.freeze([
    { id: 'fetch-asset', path: '/asset.txt', requested: '/asset.txt' },
    { id: 'fetch-prerendered', path: '/cached', requested: '/cached' },
    { id: 'fetch-rerouted', path: '/mapped', requested: '/cached' },
    { id: 'fetch-unresolved', path: '/missing', requested: '/missing' },
    { id: 'fetch-error', path: '/missing', requested: '/missing', error: true },
    { id: 'fetch-external', path: '/external', requested: '/external', external: true },
]);

export async function withRedirectSink(nonce, signal, action) {
    signal.throwIfAborted();
    const requests = [];
    const sockets = new Map();
    const server = http.createServer((request, response) => {
        if (requests.length >= 32) { response.writeHead(429); response.end(); return; }
        requests.push(request.url);
        if (request.url === '/target') response.end(`target-${nonce}`);
        else { response.writeHead(302, { location: '/target' }); response.end(`jump-${nonce}`); }
    });
    server.on('connection', (socket) => {
        const closed = new Promise((resolve) => socket.once('close', resolve));
        sockets.set(socket, closed);
    });
    const listening = new Promise((resolve, reject) => { server.once('listening', resolve); server.once('error', reject); });
    let result;
    try {
        server.listen(0, '127.0.0.1');
        await listening;
        signal.throwIfAborted();
        const address = server.address();
        result = await action({ origin: `http://127.0.0.1:${address.port}`, requests });
        signal.throwIfAborted();
    } finally {
        const closed = new Promise((resolve, reject) => server.close((error) => error && error.code !== 'ERR_SERVER_NOT_RUNNING' ? reject(error) : resolve()));
        for (const socket of sockets.keys()) socket.destroy();
        await Promise.all([closed, ...sockets.values()]);
    }
    return result;
}

export function validateFetchRows(rows, nonce) {
    assert.equal(rows.length, FETCH_CASES.length);
    assert.equal(new Set(rows.map((row) => row.id)).size, FETCH_CASES.length);
    for (const expected of FETCH_CASES) {
        const row = rows.find((item) => item.id === expected.id);
        assert.ok(row, `Missing self-fetch branch ${expected.id}.`);
        assert.deepEqual(row.requests, expected.external ? [expected.requested, '/target'] : [expected.requested]);
        assert.equal(row.httpStatus, expected.external ? 200 : 302);
        assert.equal(row.location, expected.external ? null : '/target');
        assert.equal(row.body, `${expected.external ? 'target' : 'jump'}-${nonce}`);
    }
}

export async function exerciseSelfFetch(createFetch, nonce, signal) {
    const rows = await withRedirectSink(nonce, signal, async ({ origin, requests }) => {
        const completed = [];
        for (const item of FETCH_CASES) {
            signal.throwIfAborted();
            requests.length = 0;
            const url = new URL(item.external ? 'http://fixture-context.invalid/' : origin);
            const options = { hooks: { handle: ({ event, resolve }) => resolve(event), handleFetch: ({ request, fetch }) => fetch(request),
                handleError: () => {}, reroute: ({ url }) => url.pathname === '/mapped' ? '/cached' : undefined, transport: {} },
                csrf_check_origin: true, csrf_trusted_origins: [], hash_routing: false };
            const manifest = { assets: new Set(['asset.txt']), mimeTypes: {}, _: { server_assets: {}, prerendered_routes: new Set(['/cached']),
                routes: [], matchers: async () => ({}), nodes: [] } };
            const runFetch = createFetch({ event: { url, request: new Request(url) }, options, manifest,
                state: { depth: 0, error: item.error === true }, get_cookie_header: () => '', set_internal: () => {} });
            const response = await runFetch(`${origin}${item.path}`, { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) });
            const body = await response.text();
            assert.ok(body.length <= 1024, 'Unexpected self-fetch response bound.');
            completed.push({ id: item.id, httpStatus: response.status, location: response.headers.get('location'), body, requests: [...requests] });
        }
        validateFetchRows(completed, nonce);
        return completed;
    });
    return { nonce, rows, sinkCleanupConfirmed: true };
}
