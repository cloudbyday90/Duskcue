// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import http from 'node:http';
import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import { lstat } from 'node:fs/promises';
import { installedInputs } from './prerequisite.mjs';
import { ProcessOwners, fixtureEnvironment } from './process.mjs';
import { LoopbackRequests } from './http.mjs';
import { observe } from './lifecycle.mjs';
import { applicationEntryPaths } from './application-assets.mjs';

if (process.argv.length !== 2) throw new Error('The application smoke runner accepts no overrides.');
const web = fileURLToPath(new URL('../../../clients/web/', import.meta.url));
const nonce = randomUUID();
const controller = new AbortController();
const timer = setTimeout(() => controller.abort(new Error('Application smoke deadline exceeded.')), 60000);
const interrupt = () => controller.abort(new Error('Application smoke interrupted.'));
process.on('SIGINT', interrupt); process.on('SIGTERM', interrupt);
const owners = new ProcessOwners({ signal: controller.signal });
const requests = new LoopbackRequests();
const sockets = new Map();
const seen = [];
let upstream;
let failure;
let result;
try {
    const inputs = await installedInputs(web, controller.signal);
    assert.ok((await lstat(join(web, 'build/index.js'))).isFile());
    upstream = http.createServer((request, response) => {
        if (seen.length >= 16) { response.writeHead(429); response.end(); return; }
        seen.push(request.url);
        if (request.url === '/api/v1/security-redirect') { response.writeHead(302, { location: '/auth/login' }); response.end(); }
        else if (request.url === '/api/v1/security-sse') { response.writeHead(200, { 'content-type': 'text/event-stream' }); response.end(`data: ${nonce}\n\n`); }
        else if (request.url === `/api/v1/security-smoke?echo=${nonce}`) {
            response.writeHead(200, { 'content-type': 'application/json', 'set-cookie': `security_probe=${nonce}; HttpOnly; SameSite=Strict; Path=/` });
            response.end(JSON.stringify({ nonce, forwarded: request.headers['x-forwarded-host'] }));
        } else { response.writeHead(404); response.end(); }
    });
    upstream.on('connection', (socket) => { sockets.set(socket, new Promise((resolve) => socket.once('close', resolve))); });
    await new Promise((resolve, reject) => { upstream.once('error', reject); upstream.listen(0, '127.0.0.1', resolve); });
    controller.signal.throwIfAborted();
    const backend = `http://127.0.0.1:${upstream.address().port}`;
    const server = owners.start([fileURLToPath(new URL('./worker.mjs', import.meta.url)), 'server', web, nonce], { cwd: web, nonce,
        env: fixtureEnvironment({ HOST: '127.0.0.1', PORT: '0', SHUTDOWN_TIMEOUT: '1', DUSKCUE_INTERNAL_API_URL: backend }) });
    const origin = await server.ready();
    const get = (path) => requests.get(origin, path, { signal: controller.signal });
    const login = await get('/auth/login');
    assert.equal(login.status, 200); assert.match(login.body, /<html[^>]*lang="en"/);
    assert.ok(login.body.includes('Duskcue') && !login.body.includes('%lang%') && !login.body.includes('%dir%'));
    const entries = applicationEntryPaths(login.body, origin);
    for (const path of entries) { const asset = await get(path); assert.equal(asset.status, 200); assert.ok(asset.body.length > 0); assert.match(asset.headers['content-type'], /javascript/); }
    assert.equal((await get('/bad/%')).status, 400);
    assert.equal((await get('/auth/login')).status, 200); server.assertRunning();
    const proxied = await get(`/api/v1/security-smoke?echo=${nonce}`);
    assert.equal(proxied.status, 200); assert.deepEqual(JSON.parse(proxied.body), { nonce, forwarded: new URL(origin).host });
    assert.ok(proxied.headers['set-cookie']?.some((value) => value.startsWith(`security_probe=${nonce};`)));
    const redirect = await get('/api/v1/security-redirect'); assert.equal(redirect.status, 302); assert.equal(redirect.headers.location, '/auth/login');
    const sse = await get('/api/v1/security-sse'); assert.equal(sse.status, 200); assert.equal(sse.body, `data: ${nonce}\n\n`); assert.equal(sse.headers['x-accel-buffering'], 'no');
    assert.deepEqual(seen, [`/api/v1/security-smoke?echo=${nonce}`, '/api/v1/security-redirect', '/api/v1/security-sse']);
    assert.equal((await get('/auth/login')).status, 200); server.assertRunning();
    assert.deepEqual(await installedInputs(web, controller.signal), inputs);
    result = { ssrShell: true, referencedEntryAssets: 2, malformed: true, jsonCookieProxy: true, redirect: true, sse: true, versions: inputs.versions, lockSha256: inputs.lockSha256 };
} catch (error) { failure = error; }
finally {
    const settled = await Promise.allSettled([observe(requests.close(), 10000, 'Application HTTP cleanup unconfirmed.'), owners.close(),
        upstream ? observe((async () => {
            const closed = new Promise((resolve, reject) => upstream.close((error) => error && error.code !== 'ERR_SERVER_NOT_RUNNING' ? reject(error) : resolve()));
            for (const socket of sockets.keys()) socket.destroy();
            await Promise.all([closed, ...sockets.values()]);
        })(), 10000, 'Application upstream cleanup unconfirmed.') : undefined]);
    const errors = settled.filter((item) => item.status === 'rejected').map((item) => item.reason);
    if (errors.length) failure = new AggregateError([failure, ...errors].filter(Boolean), 'Application smoke cleanup failed.');
    clearTimeout(timer); process.off('SIGINT', interrupt); process.off('SIGTERM', interrupt);
}
try { owners.assertHealthy(); controller.signal.throwIfAborted(); if (failure) throw failure; console.log(`DUSKCUE_WEB_APPLICATION_SMOKE=${JSON.stringify({ ...result, cleanupConfirmed: true })}`); }
catch (error) { console.error(error.message); if (owners.tail) console.error(owners.tail); process.exitCode = 1; }
