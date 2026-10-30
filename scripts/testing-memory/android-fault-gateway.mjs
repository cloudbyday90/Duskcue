// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createServer, request as httpRequest } from 'node:http';
import { Transform } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { once } from 'node:events';
import { boundedBody, faultRoute, fixedOrigin, requestHeaders, responseHeaders } from './android-fault-policy.mjs';

export async function createFaultGateway(fixture) {
    const origin = fixedOrigin(fixture.origin);
    if (typeof fixture.token !== 'string' || !fixture.token || fixture.token.length > 2048 || /[\r\n]/.test(fixture.token)) throw new Error('A captured fixture credential is required.');
    const sockets = new Set(), active = new Set(), upstreams = new Set();
    const trace = [], losses = new Set();
    let closing = false, failure, completion, renewArmedAt = 0;
    function refuse(response, status) { response.writeHead(status, { 'cache-control': 'no-store', connection: 'close' }); response.end(); }
    async function forward(request, response) {
        const route = faultRoute(request.method, request.url);
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
            const headers = Object.fromEntries(requestHeaders.filter((key) => request.headers[key] !== undefined).map((key) => [key, request.headers[key]]));
            headers['content-length'] = body.length;
            upstream = httpRequest({ hostname: origin.hostname, port: origin.port, path: request.url,
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
                if (shouldDrop) {
                    losses.delete(route.loss);
                    entry.dropped = true;
                    response.destroy();
                } else {
                    response.writeHead(incoming.statusCode, copied);
                    response.end(bytes);
                    await once(response, 'close');
                }
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
        if (request.headers.authorization !== `Bearer ${fixture.token}`) return refuse(response, 401);
        try { faultRoute(request.method, request.url); } catch { return refuse(response, 404); }
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
