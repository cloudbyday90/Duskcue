// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createServer, request as upstreamRequest } from 'node:http';

export async function startVodGateway(origin, backend) {
    const publicUrl = new URL(origin);
    const target = new URL(backend);
    if (publicUrl.origin !== origin || publicUrl.protocol !== 'http:' || !/^127\.\d+\.\d+\.\d+$/.test(publicUrl.hostname)
        || publicUrl.port !== '48027' || target.protocol !== 'http:' || target.hostname !== '127.0.0.1' || !target.port
        || target.username || target.password || target.pathname !== '/' || target.search || target.hash) throw new Error('Owned loopback origins required');
    const records = [];
    const sockets = new Set();
    const upstreams = new Set();
    const server = createServer((request, response) => {
        if (upstreams.size >= 32 || records.length >= 2000 || !['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'].includes(request.method)
            || typeof request.url !== 'string' || request.url.length > 4096 || !/^\/(?:api\/v1\/|health(?:\/|$))/.test(request.url)
            || request.url.startsWith('//') || request.headers.host !== publicUrl.host) { response.writeHead(400); response.end(); return; }
        const record = { method: request.method, path: new URL(request.url, origin).pathname,
            bearer: /^Bearer [A-Za-z0-9_-]{32,256}$/.test(request.headers.authorization || ''), cookie: !!request.headers.cookie, status: 0 };
        records.push(record);
        const upstream = upstreamRequest(new URL(request.url, target), { method: request.method,
            headers: { ...request.headers, host: target.host }, timeout: 60_000 }, (incoming) => {
            record.status = incoming.statusCode;
            response.writeHead(incoming.statusCode, incoming.headers);
            let bytes = 0;
            incoming.on('data', (chunk) => { bytes += chunk.length; if (bytes > 16 * 1024 * 1024 && record.path !== '/api/v1/events') incoming.destroy(new Error('Owned response exceeds bound')); });
            incoming.on('error', () => response.destroy());
            incoming.pipe(response);
        });
        upstreams.add(upstream);
        let bytes = 0;
        const body = [];
        request.on('data', (chunk) => { bytes += chunk.length; if (bytes > 32768) { request.destroy(); upstream.destroy(); }
            else if (request.method === 'POST' && record.path.endsWith('/configuration')) body.push(chunk); });
        request.on('end', () => {
            if (!body.length) return;
            try { const value = JSON.parse(Buffer.concat(body).toString('utf8')); record.action = value.action?.kind; record.evidenceKind = value.action?.evidence?.kind; }
            catch {}
            body.length = 0;
        });
        upstream.on('timeout', () => upstream.destroy());
        upstream.on('error', () => { if (!response.headersSent) response.writeHead(502); response.end(); });
        upstream.on('close', () => upstreams.delete(upstream));
        response.on('close', () => upstream.destroy());
        request.pipe(upstream);
    });
    server.on('connection', (socket) => { sockets.add(socket); socket.once('close', () => sockets.delete(socket)); if (sockets.size > 32) socket.destroy(); });
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(48027, publicUrl.hostname, resolve); });
    return { records, async close() {
        for (const upstream of upstreams) upstream.destroy();
        for (const socket of sockets) socket.destroy();
        await new Promise((resolve) => server.close(resolve));
    } };
}
