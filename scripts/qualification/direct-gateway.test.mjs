// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validateFixtureRequest, fixtureResponseHeaders } from '../../docker/qualification/direct-http-policy.mjs';
import { startDirectGateway } from './direct-gateway.mjs';

const request = (path, extra = {}) => ({ method: 'GET', path, headers: {}, body: '', ...extra });

test('fixed upstream policy rejects routing escapes, credentials in queries and unbounded inputs', () => {
    for (const path of ['https://example.com/api/v1/events', '//example.com/api/v1/events', '/api/v1/../metrics', '/api/v1/%2e%2e/metrics', '/api/v1/events#x', '/api/v1/events?token=secret', '/api/v1/events?x=%00', '/api/v1/events?x=%GG', '/api/v1/events?x=1?y=2']) assert.throws(() => validateFixtureRequest(request(path)));
    assert.throws(() => validateFixtureRequest(request('/api/v1/events', { headers: { host: 'example.com' } })));
    assert.throws(() => validateFixtureRequest(request('/api/v1/events', { headers: { origin: 'a\r\nb' } })));
    assert.throws(() => validateFixtureRequest(request('/api/v1/events', { method: 'CONNECT' })));
    assert.throws(() => validateFixtureRequest(request('/api/v1/events', { body: 'unexpected' })));
    assert.throws(() => validateFixtureRequest(request('/api/v1/profiles/current/viewing-preferences', { method: 'PATCH', body: 'x'.repeat(65537) })));
    assert.equal(validateFixtureRequest(request('/api/v1/items/abc/storyboard?media_file_id=123&profile_id=456')).path, '/api/v1/items/abc/storyboard?media_file_id=123&profile_id=456');
});

test('gateway preserves production CORS and security headers without compression framing', async () => {
    const headers = fixtureResponseHeaders(new Headers({ 'access-control-allow-origin': 'http://tauri.localhost', 'x-request-id': 'fixture-request', 'x-content-type-options': 'nosniff', 'content-encoding': 'gzip', 'content-length': '999', 'set-cookie': 'session=ephemeral; HttpOnly; SameSite=Strict' }));
    const calls = [];
    const gateway = await startDirectGateway({ bridge: { async request(message) { calls.push(message); return { status: 200, headers, body: Buffer.from('{}').toString('base64') }; } } });
    try {
        const response = await fetch(`${gateway.origin}/api/v1/profiles?limit=1`, { headers: { Authorization: 'Bearer ephemeral', Origin: 'http://tauri.localhost' } });
        assert.equal(await response.text(), '{}');
        assert.equal(response.headers.get('access-control-allow-origin'), 'http://tauri.localhost');
        assert.equal(response.headers.get('x-content-type-options'), 'nosniff');
        assert.equal(response.headers.get('x-request-id'), 'fixture-request');
        assert.equal(calls[0].headers.authorization, 'Bearer ephemeral');
        assert.equal(headers['content-encoding'], undefined);
        assert.equal(headers['content-length'], undefined);
        const refused = await fetch(`${gateway.origin}/api/v1/events?token=secret`);
        assert.equal(refused.status, 502);
        assert.equal(calls.length, 1);
        assert.ok(!JSON.stringify(gateway.requests).includes('ephemeral'));
    } finally { await gateway.close(); }
});

test('gateway forwards actual event bytes and cancels an owned stream when its consumer closes', async () => {
    let cancelled = false;
    let finish;
    const gateway = await startDirectGateway({ bridge: { stream(_message, emit) {
        const done = new Promise((resolve) => { finish = resolve; });
        queueMicrotask(() => { emit({ type: 'headers', status: 200, headers: { 'content-type': 'text/event-stream' } }); emit({ type: 'chunk', body: Buffer.from('event: ready\ndata: {}\n\n').toString('base64') }); });
        return { done, async cancel() { cancelled = true; finish(); } };
    } } });
    try {
        const controller = new AbortController();
        const response = await fetch(`${gateway.origin}/api/v1/events`, { signal: controller.signal });
        assert.equal(response.headers.get('content-type'), 'text/event-stream');
        const reader = response.body.getReader();
        assert.equal(new TextDecoder().decode((await reader.read()).value), 'event: ready\ndata: {}\n\n');
        controller.abort();
        await reader.cancel().catch(() => {});
        for (let index = 0; index < 20 && !cancelled; index += 1) await new Promise((resolve) => setTimeout(resolve, 10));
        assert.equal(cancelled, true);
    } finally { finish?.(); await gateway.close(); }
});
