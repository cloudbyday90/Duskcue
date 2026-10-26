/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { afterEach, expect, it, vi } from 'vitest';
import { get, invalidateProfileScopedRequests, clearBearerToken, clearServerOrigin } from '../../src/lib/api/core.js';
import { boundedJson } from '../../src/lib/api/json-body.js';
import { retryAfterSeconds } from '../../src/lib/api/retry-after.js';

afterEach(() => { vi.unstubAllGlobals(); invalidateProfileScopedRequests(); clearBearerToken(); clearServerOrigin(); });

it('counts actual UTF-8 stream bytes and cancels overflow even with a false Content-Length', async () => {
    const cancelled = vi.fn();
    let reads = 0;
    const stream = new ReadableStream({ pull(controller) { reads++; controller.enqueue(new TextEncoder().encode('🙂')); }, cancel: cancelled });
    await expect(boundedJson(new Response(stream, { headers: { 'Content-Length': '1' } }), 7, null)).rejects.toThrow('budget');
    expect(cancelled).toHaveBeenCalledOnce();
    expect(reads).toBeLessThanOrEqual(3);
});

it('rejects oversized declared bodies, malformed UTF-8 and malformed JSON', async () => {
    await expect(boundedJson(new Response('{}', { headers: { 'Content-Length': '99' } }), 8, null)).rejects.toThrow();
    await expect(boundedJson(new Response(new Uint8Array([0xff])), 8, null)).rejects.toThrow();
    await expect(boundedJson(new Response('{'), 8, null)).rejects.toThrow();
});

it('keeps profile cancellation through a pending bounded body read', async () => {
    const cancelled = vi.fn();
    const stream = new ReadableStream({ start(controller) { controller.enqueue(new TextEncoder().encode('{')); }, cancel: cancelled });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(stream, { headers: { 'Content-Type': 'application/json' } })));
    const pending = get('/live-tv/catalog/channels', {}, { maxJsonBytes: 1024 });
    const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    await vi.waitFor(() => expect(fetch).toHaveBeenCalledOnce());
    invalidateProfileScopedRequests();
    await rejected;
    expect(cancelled).toHaveBeenCalledOnce();
});

it('uses fixed HTTP status and Retry-After for malformed or hostile error bodies', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => new Response('{"status":200,"detail":"private"}', { status: 503, headers: { 'Content-Type': 'application/problem+json', 'Retry-After': '5' } })));
    await expect(get('/live-tv/catalog/channels', {}, { maxJsonBytes: 16, cache: 'no-store' })).rejects.toMatchObject({ status: 503, retryAfter: 5, message: 'HTTP_503', detail: '' });
    expect(fetch).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ cache: 'no-store', credentials: 'same-origin' }));
});

it('accepts delta seconds and HTTP dates without treating arbitrary prefixes as seconds', () => {
    const now = Date.parse('Mon, 26 Oct 2026 12:00:00 GMT');
    expect(retryAfterSeconds('Mon, 26 Oct 2026 12:00:05 GMT', now)).toBe(5);
    expect(retryAfterSeconds('Monday, 26-Oct-26 12:00:05 GMT', now)).toBe(5);
    expect(retryAfterSeconds('Mon Oct 26 12:00:05 2026', now)).toBe(5);
    expect(retryAfterSeconds('0')).toBe(0);
    for (const value of ['5bad', '-1', '1.5', '9999999999999999999999', 'tomorrow', 'Tue, 26 Oct 2026 12:00:05 GMT', 'Mon, 30 Feb 2026 12:00:05 GMT', 'Mon, 26 Oct 2026 24:00:00 GMT']) expect(retryAfterSeconds(value, now)).toBeNull();
});

it('interprets RFC 850 years against the fifty-year boundary and keeps leap seconds explicit', () => {
    const now = Date.parse('2026-10-26T12:00:00Z');
    const future = new Date('2075-01-01T00:00:00Z');
    const weekday = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'][future.getUTCDay()];
    expect(retryAfterSeconds(`${weekday}, 01-Jan-75 00:00:00 GMT`, now)).toBe((future.getTime() - now) / 1000);
    expect(retryAfterSeconds('Sunday, 06-Nov-94 08:49:37 GMT', now)).toBe(0);
    expect(retryAfterSeconds('Sun Nov  6 08:49:37 1994', now)).toBe(0);
    expect(retryAfterSeconds('Sat, 31 Dec 2016 23:59:60 GMT', Date.parse('2016-12-31T23:59:59Z'))).toBe(1);
});
