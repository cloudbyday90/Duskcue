// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const REQUEST_HEADERS = new Set(['accept', 'cookie', 'content-type', 'range', 'authorization', 'origin', 'if-none-match', 'last-event-id', 'access-control-request-method', 'access-control-request-headers']);
export const RESPONSE_HEADERS = new Set(['content-type', 'content-range', 'accept-ranges', 'cache-control', 'etag', 'vary', 'set-cookie', 'x-request-id', 'x-content-type-options', 'content-security-policy', 'access-control-allow-origin', 'access-control-allow-methods', 'access-control-allow-headers', 'access-control-allow-credentials', 'access-control-expose-headers', 'retry-after']);

export function validateFixtureRequest(message) {
    if (!['GET', 'HEAD', 'POST', 'PATCH', 'DELETE', 'OPTIONS'].includes(message.method)
        || typeof message.path !== 'string' || message.path.length > 4096 || /[#\\\s\x00-\x1f]/.test(message.path)
        || typeof message.body !== 'string' || Buffer.byteLength(message.body) > 65536) throw new Error('Invalid fixture HTTP request.');
    const [path, query = ''] = message.path.split('?');
    if (message.path.split('?').length > 2 || !/^(?:\/api\/v1\/[A-Za-z0-9/_-]+(?:\.(?:m3u8|m4s|mp4|vtt|jpg|webp))?|\/health\/(?:live|ready))$/.test(path)
        || path.includes('//') || query.length > 2048 || /%(?![a-fA-F0-9]{2})/.test(query)) throw new Error('Invalid fixture path.');
    const params = [...new URLSearchParams(query)];
    if (params.length > 16 || params.some(([key, value]) => !/^[a-zA-Z0-9_]{1,64}$/.test(key) || value.length > 512 || /[\x00-\x1f\x7f]/.test(value) || /token|password|authorization|secret/i.test(key))) throw new Error('Invalid fixture query.');
    if (['GET', 'HEAD', 'OPTIONS'].includes(message.method) && message.body) throw new Error('Unexpected fixture body.');
    const headers = {};
    if (!message.headers || typeof message.headers !== 'object' || Array.isArray(message.headers) || Object.keys(message.headers).length > REQUEST_HEADERS.size) throw new Error('Invalid fixture headers.');
    for (const [key, value] of Object.entries(message.headers)) {
        if (!REQUEST_HEADERS.has(key) || typeof value !== 'string' || value.length > 4096 || /[\r\n\x00]/.test(value)) throw new Error('Invalid fixture header.');
        headers[key] = value;
    }
    return { path: message.path, method: message.method, headers, ...(!['GET', 'HEAD', 'OPTIONS'].includes(message.method) ? { body: message.body } : {}) };
}

export function fixtureResponseHeaders(headers) {
    const result = Object.fromEntries([...headers].filter(([key]) => RESPONSE_HEADERS.has(key)));
    if (headers.getSetCookie?.().length) result['set-cookie'] = headers.getSetCookie();
    return result;
}
