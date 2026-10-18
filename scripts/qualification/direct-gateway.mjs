// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { REQUEST_HEADERS, validateFixtureRequest } from '../../docker/qualification/direct-http-policy.mjs';

export async function startDirectGateway({ bridge, assets, host = '127.0.0.1', port = 0, requests = [] }) {
    if (!/^127(?:\.\d{1,3}){3}$/.test(host) || !Number.isSafeInteger(port) || port < 0 || port > 65535) throw new Error('An owned loopback gateway is required.');
    const operations = new Set();
    const streams = new Set();
    const errors = [];
    const html = '<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Owned live Player qualification</title><link rel="icon" href="data:,"><link rel="stylesheet" href="/direct.css"><style>html,body{margin:0;width:100%;height:100%}[data-direct-player-root]{width:100%;height:100dvh}#fixture-audio{position:fixed;z-index:20}</style></head><body><button id="fixture-audio">Enable fixture audio</button><main data-direct-player-root></main><script type="module" src="/direct.js"></script></body></html>';
    const server = createServer((request, response) => {
        const task = handle(request, response).catch(() => {
            errors.push('Owned fixture request failed.');
            if (!response.headersSent) response.writeHead(502);
            response.end();
        }).finally(() => operations.delete(task));
        operations.add(task);
    });
    async function handle(request, response) {
        if (operations.size > 16) throw new Error('Gateway request bound exceeded.');
        if (request.method === 'GET' && request.url === '/' && assets) {
            response.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store', 'content-security-policy': "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; media-src 'self' blob:; connect-src 'self'; worker-src blob:" });
            response.end(html); return;
        }
        if (request.method === 'GET' && /^\/[A-Za-z0-9_.-]+\.(js|css)$/.test(request.url || '') && assets) {
            const body = await readFile(join(assets, request.url.slice(1)));
            if (body.length > 8388608) throw new Error('Gateway asset bound exceeded.');
            response.writeHead(200, { 'content-type': request.url.endsWith('.css') ? 'text/css' : 'text/javascript', 'cache-control': 'no-store' }); response.end(body); return;
        }
        let bytes = 0;
        const chunks = [];
        for await (const chunk of request) { bytes += chunk.length; if (bytes > 65536) throw new Error('Gateway body bound exceeded.'); chunks.push(chunk); }
        const headers = Object.fromEntries(Object.entries(request.headers).filter(([key]) => REQUEST_HEADERS.has(key)));
        const message = { path: request.url, method: request.method, headers, body: Buffer.concat(chunks).toString('utf8') };
        validateFixtureRequest(message);
        if (requests.length >= 2000) throw new Error('Gateway evidence bound exceeded.');
        const record = { method: request.method, path: request.url, cookie: typeof headers.cookie === 'string', bearer: /^Bearer [A-Za-z0-9_-]+$/.test(headers.authorization || '') };
        requests.push(record);
        if (request.url === '/api/v1/events' && request.method === 'GET') {
            let stream;
            const close = () => { void Promise.resolve().then(() => stream?.cancel()).catch(() => {}); };
            response.once('close', close);
            stream = bridge.stream(message, (data) => {
                if (data.type === 'headers') { record.status = data.status; response.writeHead(data.status, data.headers); response.flushHeaders(); }
                else if (data.type === 'chunk') {
                    if (!response.write(Buffer.from(data.body, 'base64')) && response.writableLength > 262144) throw new Error('Gateway event consumer exceeded its bound.');
                } else throw new Error('Unexpected gateway stream data.');
            });
            streams.add(stream);
            try { await stream.done; }
            finally { streams.delete(stream); response.off('close', close); response.end(); }
            return;
        }
        const upstream = await bridge.request({ operation: 'fetch', ...message });
        record.status = upstream.status;
        response.writeHead(upstream.status, upstream.headers);
        response.end(Buffer.from(upstream.body, 'base64'));
    }
    await new Promise((resolve, reject) => { server.once('error', reject); server.listen(port, host, resolve); });
    return {
        origin: `http://${host}:${server.address().port}`, requests, errors,
        async close() {
            await Promise.allSettled([...streams].map((stream) => Promise.resolve().then(() => stream.cancel())));
            server.closeAllConnections();
            await new Promise((resolve) => server.close(resolve));
            let timer;
            try { await Promise.race([Promise.all([...operations]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Gateway operations did not settle.')), 10000); })]); }
            finally { clearTimeout(timer); }
        },
    };
}
