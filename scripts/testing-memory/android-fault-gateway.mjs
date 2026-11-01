// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createServer, request as httpRequest } from 'node:http';
import { Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { once } from 'node:events';
import { boundedBody, faultRoute, fixedOrigin, requestHeaders, responseHeaders } from './android-fault-policy.mjs';

export async function createFaultGateway(fixture, { scopeTokens, relayWitness } = {}) {
    const origin = fixedOrigin(fixture.origin);
    if (typeof fixture.token !== 'string' || !fixture.token || fixture.token.length > 2048 || /[\r\n]/.test(fixture.token)) throw new Error('A captured fixture credential is required.');
    const credentials = scopeTokens ?? [fixture.token];
    if (!Array.isArray(credentials) || credentials.length > 3 || credentials.some((token) => typeof token !== 'string' || !token || token.length > 2048 || /[\r\n]/.test(token))) throw new Error('Invalid fixed qualification credentials.');
    const sockets = new Set(), active = new Set(), upstreams = new Set();
    const trace = [], losses = new Set();
    let closing = false, failure, completion, renewArmedAt = 0;
    let armed = false, held = false, cancelled = false, release;
    const routeFor = (method, path) => {
        if (relayWitness && method === 'POST' && path === '/api/v1/qualification/relay-output') return { media: false };
        if (scopeTokens && (method === 'POST' && (/^\/api\/v1\/profiles\/[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\/switch$/.test(path) || path === '/api/v1/auth/logout') || path === '/qualification/scope' && ['GET', 'POST'].includes(method))) return { media: false };
        return faultRoute(method, path);
    };
    function refuse(response, status) { response.writeHead(status, { 'cache-control': 'no-store', connection: 'close' }); response.end(); }
    async function forward(request, response) {
        const route = routeFor(request.method, request.url);
        const entry = { method: request.method, path: request.url, status: null, complete: false, dropped: false };
        trace.push(entry);
        const controller = new AbortController();
        const abort = () => controller.abort();
        response.once('close', abort);
        const deadline = setTimeout(abort, 30000);
        let upstream, ended;
        try {
            const body = await boundedBody(request, 32768);
            if (request.method === 'GET' && body.length) throw new Error('GET body is outside qualification.');
            if (relayWitness && request.url === '/api/v1/qualification/relay-output') {
                const result = await relayWitness.probe(JSON.parse(body.toString('utf8')), request.headers, controller.signal);
                response.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
                response.end(JSON.stringify(result));
                await once(response, 'close'); entry.complete = true; entry.status = 200; return;
            }
            if (scopeTokens && request.url === '/qualification/scope') {
                if (request.method === 'POST') {
                    const action = JSON.parse(body.toString('utf8')).action;
                    if (action === 'arm' && !armed && !held && !cancelled) armed = true;
                    else if (action === 'release' && held) release?.();
                    else throw new Error('Invalid scope witness transition.');
                }
                response.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
                response.end(JSON.stringify({ armed, held, cancelled }));
                await once(response, 'close');
                entry.complete = true;
                return;
            }
            const headers = Object.fromEntries(requestHeaders.filter((key) => request.headers[key] !== undefined).map((key) => [key, request.headers[key]]));
            headers['content-length'] = body.length;
            upstream = httpRequest({ hostname: origin.hostname, port: origin.port, path: relayWitness?.path(request.url) ?? request.url,
                method: request.method, headers, agent: false, signal: controller.signal, maxHeaderSize: 8192 });
            upstreams.add(upstream);
            ended = new Promise((resolve) => upstream.once('close', resolve));
            const received = once(upstream, 'response');
            upstream.end(body);
            const [incoming] = await received;
            incoming.on('error', () => {});
            entry.status = incoming.statusCode;
            const shouldDrop = route.loss && losses.has(route.loss) && incoming.statusCode >= 200 && incoming.statusCode < 300;
            const copied = Object.fromEntries(responseHeaders.filter((key) => incoming.headers[key] !== undefined).map((key) => [key, incoming.headers[key]]));
            copied['cache-control'] = 'no-store';
            if (!route.media) {
                const bytes = await boundedBody(incoming, 32768);
                entry.complete = true;
                if (scopeTokens && armed && request.method === 'GET' && /^\/api\/v1\/playback-commands\/[0-9a-f-]{36}$/.test(request.url)) {
                    armed = false; held = true;
                    await new Promise((resolve) => {
                        release = resolve;
                        controller.signal.addEventListener('abort', resolve, { once: true });
                        if (controller.signal.aborted) resolve();
                    });
                    cancelled = controller.signal.aborted;
                    entry.cancelled = cancelled;
                    held = false; release = undefined;
                    if (cancelled) return;
                }
                if (shouldDrop) {
                    losses.delete(route.loss);
                    entry.dropped = true;
                    response.destroy();
                } else {
                    response.writeHead(incoming.statusCode, copied);
                    response.end(bytes);
                    await once(response, 'close');
                }
            } else if (relayWitness && request.url.endsWith('.m3u8') && incoming.statusCode === 200) {
                const bytes = await boundedBody(incoming, 1048576);
                const published = relayWitness.manifest(request.url, bytes);
                delete copied['content-length'];
                response.writeHead(200, { ...copied, 'content-length': published.length });
                response.end(published); await once(response, 'close'); entry.complete = true;
            } else {
                let bytes = 0;
                const limit = new Transform({ transform(chunk, encoding, callback) {
                    bytes += chunk.length;
                    callback(bytes > 16777216 ? new Error('Qualification media exceeded its bound.') : null, chunk);
                } });
                response.writeHead(incoming.statusCode, copied);
                await pipeline(incoming, limit, response);
                if (!incoming.complete) throw new Error('Incomplete upstream media.');
                entry.complete = true;
            }
        } finally {
            clearTimeout(deadline);
            response.removeListener('close', abort);
            upstream?.destroy();
            await ended;
            upstreams.delete(upstream);
            if (!response.closed) {
                const closed = once(response, 'close');
                response.destroy();
                await closed;
            }
        }
    }
    const server = createServer({ maxHeaderSize: 8192, headersTimeout: 5000, requestTimeout: 30000 }, (request, response) => {
        if (closing || failure || active.size >= 8 || trace.length >= 2048) return refuse(response, 503);
        if (!credentials.some((token) => request.headers.authorization === `Bearer ${token}`)) return refuse(response, 401);
        try { routeFor(request.method, request.url); } catch { return refuse(response, 404); }
        const work = forward(request, response);
        active.add(work);
        work.catch(() => { response.destroy(); }).finally(() => active.delete(work));
    });
    server.on('connection', (socket) => {
        const full = sockets.size >= 16;
        sockets.add(socket);
        socket.once('close', () => sockets.delete(socket));
        socket.on('error', () => {});
        if (closing || full) socket.destroy();
    });
    server.on('error', (error) => { failure = error; });
    server.on('clientError', (_, socket) => socket.destroy());
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', resolve); });
    return {
        origin: `http://127.0.0.1:${server.address().port}`, trace,
        verifyScope(starts, requireCancellation) {
            if (!scopeTokens || failure || active.size || armed || held) throw new Error('Scope witness did not settle.');
            const entries = trace.filter((entry) => entry.method === 'POST' && entry.path === '/api/v1/playback-commands');
            if (entries.length !== starts || entries.some((entry) => !entry.complete || entry.status !== 202)) throw new Error('Scope Start attribution differs.');
            if (requireCancellation && !cancelled) throw new Error('Scope HTTP cancellation was not observed.');
        },
        arm({ deferRenew = false } = {}) {
            if (closing || losses.size || trace.length) throw new Error('Fault witness must arm once before its stage.');
            for (const key of ['start', 'change', 'withdraw', ...deferRenew ? [] : ['renew']]) losses.add(key);
            renewArmedAt = deferRenew ? null : 0;
        },
        armRenew() {
            if (closing || renewArmedAt !== null) throw new Error('Renewal fault phase was repeated or unowned.');
            renewArmedAt = trace.length;
            losses.add('renew');
        },
        verify() {
            if (failure || losses.size || active.size) throw new Error('Qualification faults did not settle.');
            for (const key of ['start', 'change', 'renew', 'withdraw']) {
                const entries = trace.slice(key === 'renew' ? renewArmedAt ?? trace.length : 0).filter((entry) => faultRoute(entry.method, entry.path).loss === key);
                if (entries.length !== 1 || !entries[0].complete || !entries[0].dropped) throw new Error('Mutation was missing, repeated or not conclusively lost.');
            }
            if (!trace.some((entry) => entry.method === 'GET' && entry.path.endsWith('/settlement') && entry.complete)) throw new Error('Withdrawal was not explicitly reconciled.');
        },
        close() {
            return completion ??= (async () => {
                closing = true;
                const stopped = new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
                for (const upstream of upstreams) upstream.destroy();
                const closed = [...sockets].map((socket) => new Promise((resolve) => { socket.once('close', resolve); socket.destroy(); }));
                await Promise.all([stopped, ...closed, ...[...active].map((work) => work.catch(() => {}))]);
                if (active.size || sockets.size || upstreams.size || server.listening) throw new Error('Fault witness cleanup is unconfirmed.');
                if (failure) throw failure;
            })();
        },
    };
}
