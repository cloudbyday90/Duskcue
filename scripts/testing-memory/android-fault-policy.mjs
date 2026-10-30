// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const uuid = '[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}';
const command = new RegExp(`^/api/v1/playback-commands/${uuid}(/.*)?$`);
const item = new RegExp(`^/api/v1/(?:items/${uuid}/watch-data|media-items/${uuid}/files)$`);
const operation = new RegExp(`^/operations/${uuid}$`);
const media = new RegExp(`^/outputs/${uuid}/hls/(?:manifest\\.m3u8|[A-Za-z0-9_-]{1,64}/(?:index\\.m3u8|init\\.mp4|seg_[0-9]{1,50}\\.m4s))$`);

export function faultRoute(method, path) {
    if (method === 'GET' && (path === '/api/v1/auth/session-context' || item.test(path))) return { media: false };
    if (method === 'POST' && path === '/api/v1/playback/prepare') return { media: false };
    if (method === 'POST' && path === '/api/v1/playback-commands') return { media: false, loss: 'start' };
    const match = command.exec(path);
    if (!match) throw new Error('Qualification request is outside the fixed route set.');
    const suffix = match[1] ?? '';
    if (method === 'GET' && (['', '/configuration', '/checkpoint', '/settlement'].includes(suffix) || operation.test(suffix))) return { media: false };
    if (method === 'GET' && media.test(suffix)) return { media: true };
    if (method === 'POST' && ['/configuration', '/renew', '/withdraw', '/checkpoint'].includes(suffix)) {
        return { media: false, loss: { '/configuration': 'change', '/renew': 'renew', '/withdraw': 'withdraw' }[suffix] };
    }
    throw new Error('Qualification request is outside the fixed route set.');
}

export function fixedOrigin(origin) {
    const url = new URL(origin);
    if (url.origin !== origin || url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || !url.port) throw new Error('An owned literal loopback HTTP origin is required.');
    return url;
}

export async function boundedBody(stream, limit) {
    const parts = [];
    let size = 0;
    for await (const part of stream) {
        size += part.length;
        if (size > limit) throw new Error('Qualification message exceeded its byte bound.');
        parts.push(part);
    }
    if (!stream.complete) throw new Error('Qualification message did not complete.');
    return Buffer.concat(parts, size);
}

export const requestHeaders = ['authorization', 'x-session-context', 'content-type', 'accept', 'if-match', 'range', 'accept-encoding', 'cache-control'];
export const responseHeaders = ['content-length', 'content-type', 'content-range', 'accept-ranges', 'etag', 'location', 'retry-after', 'content-encoding'];
