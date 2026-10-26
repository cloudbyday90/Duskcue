/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createCommandOwner } from '../../src/lib/live-tv-management/commands.js';
import { createRefreshMonitor } from '../../src/lib/live-tv-management/monitor.js';
import { exactReviver, accepted, assignment, resource, parseCap, etag } from '../../src/lib/live-tv-management/schema.js';
import { sourcePair, destination } from '../../src/lib/live-tv-management/forms.js';
import { fixtureAssignment, managementIds } from '../fixtures/live-tv-management.js';
import { get, invalidateProfileScopedRequests } from '../../src/lib/api/core.js';

afterEach(() => { vi.unstubAllGlobals(); vi.useRealTimers(); });
function deferred() { let resolve!: (value?: any) => void; const promise = new Promise<any>((done) => { resolve = done; }); return { promise, resolve }; }

describe('exact management metadata', () => {
    it('recovers i64 source text above 2^53 without changing counts or backend wire contracts', () => {
        const body = JSON.parse('{"revision":9007199254740993,"source_generation":9223372036854775807,"programme_count":3}', exactReviver);
        expect(body).toEqual({ revision: '9007199254740993', source_generation: '9223372036854775807', programme_count: 3 });
        expect(() => JSON.parse('{"revision":9223372036854775808}', exactReviver)).toThrow();
        expect(() => JSON.parse('{"revision":1e3}', exactReviver)).toThrow();
        expect(exactReviver('revision', 7)).toBe('7');
        expect(() => exactReviver('revision', 9007199254740992)).toThrow();
    });
    it('keeps resource identities and validator kinds separate', () => {
        const body = JSON.parse(JSON.stringify({ ...fixtureAssignment(managementIds.target), management_scope: 'administrator' }), exactReviver);
        const saved = assignment(body, managementIds.target);
        expect(() => assignment(body, managementIds.control)).toThrow();
        expect(() => assignment(body, managementIds.target, true)).toThrow();
        expect(() => resource({ ...saved, assignment_id: managementIds.snapshot }, saved)).toThrow();
        expect(() => etag(`W/"${'a'.repeat(64)}"`)).toThrow();
    });
    it('never follows a status URL outside the exact selected household and accepted UUIDv7 attempt', () => {
        const status_url = `/api/v1/live-tv/households/${managementIds.target}/refresh/status?attempt_id=${managementIds.attempt}`;
        const body = { attempt_id: managementIds.attempt, attempt_sequence: '1', status_url };
        expect(accepted(body, status_url, managementIds.target).attempt_id).toBe(managementIds.attempt);
        for (const other of [`https://evil.invalid${status_url}`, `${status_url}&extra=1`, status_url.replace(managementIds.target, managementIds.control)]) expect(() => accepted({ ...body, status_url: other }, other, managementIds.target)).toThrow();
        expect(() => accepted(body, '/api/v1/other', managementIds.target)).toThrow();
    });
    it('preserves opaque source URL bytes and ordered explicit pins without provider requests', () => {
        const raw = 'https://dispatcharr.invalid/prefix/m3u/?password=a%2Fb&x=1';
        expect(sourcePair(raw, raw).playlist_url).toBe(raw);
        for (const input of ['https://u:p@dispatcharr.invalid/', raw + '#secret', raw + ' ', 'file:///tmp/source', 'https://d.invalid/\\path']) expect(() => sourcePair(input, raw)).toThrow();
        expect(destination('http://127.0.0.1:48027/', '127.0.0.2\n127.0.0.1').addresses).toEqual(['127.0.0.2', '127.0.0.1']);
        expect(() => destination('http://127.0.0.1/path', '127.0.0.1')).toThrow();
        expect(() => destination('http://127.0.0.1/', '127.0.0.1\n127.0.0.1')).toThrow();
        expect(() => destination('http://127.0.0.1/', 'localhost')).toThrow();
        expect(parseCap('')).toBeNull(); expect(parseCap('2147483647')).toBe(2147483647);
        for (const input of ['0', '-1', '1e3', '1.5', '2147483648']) expect(() => parseCap(input)).toThrow();
    });
});

describe('bounded body and header ownership', () => {
    it('retains scope cancellation after headers until streamed JSON is complete', async () => {
        let stream: ReadableStreamDefaultController<Uint8Array>;
        let cancelled = false;
        const body = new ReadableStream<Uint8Array>({ start(controller) { stream = controller; }, cancel() { cancelled = true; } });
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(body, { headers: { 'Content-Type': 'application/json', ETag: `"${'a'.repeat(64)}"` } })));
        const pending = get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true });
        const failure = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
        await vi.waitFor(() => expect(body.locked).toBe(true));
        invalidateProfileScopedRequests(); await failure;
        expect(cancelled).toBe(true);
    });
    it('captures a validated bounded body and selected headers without bypassing errors', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{"revision":9007199254740993}', { headers: { 'Content-Type': 'application/json', ETag: `"${'a'.repeat(64)}"` } })));
        expect(await get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true, jsonReviver: exactReviver })).toMatchObject({ status: 200, body: { revision: '9007199254740993' }, etag: `"${'a'.repeat(64)}"` });
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{"title":"PRIVATE_DIAGNOSTIC","detail":"private"}', { status: 403, headers: { 'Content-Type': 'application/json' } })));
        await expect(get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true })).rejects.toMatchObject({ status: 403, title: 'HTTP_403', detail: '' });
    });
    it('refuses oversized bodies and selected metadata before returning an editable result', async () => {
        for (const response of [
            new Response('{"value":"' + 'x'.repeat(1024) + '"}', { headers: { 'Content-Type': 'application/json' } }),
            new Response('{}', { headers: { 'Content-Type': 'application/json', ETag: 'x'.repeat(257) } }),
            new Response('{}', { headers: { 'Content-Type': 'application/json', Location: 'x'.repeat(513) } }),
        ]) {
            vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response));
            await expect(get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true })).rejects.toThrow();
        }
    });
    it('cancels an unexpected success body without buffering it or allowing a raw text bypass', async () => {
        let cancelled = false;
        const body = new ReadableStream({ cancel() { cancelled = true; } });
        const fetch = vi.fn().mockResolvedValue(new Response(body, { headers: { 'Content-Type': 'text/html' } }));
        vi.stubGlobal('fetch', fetch);
        await expect(get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true })).rejects.toThrow('media type');
        expect(cancelled).toBe(true);
        await expect(get('/live-tv/fixture', {}, { maxJsonBytes: 1024, resultMetadata: true, responseType: 'text' })).rejects.toThrow('bounded response mode');
        expect(fetch).toHaveBeenCalledOnce();
    });
});

describe('retained management commands', () => {
    it('rejects overlap, retains submitted work through navigation, and does not repeat it', async () => {
        let current = true; const auth = { credentialWork: () => null, isCurrent: () => current }; const owner = createCommandOwner(auth); const gate = deferred(); const submit = vi.fn(() => gate.promise);
        const pending = owner.run({}, submit); await vi.waitFor(() => expect(submit).toHaveBeenCalledOnce());
        await expect(owner.run({}, submit)).rejects.toThrow(); current = false;
        expect(owner.pending).toBe(pending); gate.resolve('accepted'); await expect(pending).resolves.toBe('accepted');
        expect(submit).toHaveBeenCalledOnce(); expect(owner.pending).toBeNull();
    });
    it('refuses a superseded context before dispatch and pending credential adoption', async () => {
        const submit = vi.fn(); const auth = { credentialWork: () => null, isCurrent: () => false }; const owner = createCommandOwner(auth);
        await expect(owner.run({}, submit)).rejects.toThrow(); expect(submit).not.toHaveBeenCalled();
        const adoption = createCommandOwner({ credentialWork: () => Promise.resolve(), isCurrent: () => true });
        await expect(adoption.run({}, submit)).rejects.toThrow(); expect(submit).not.toHaveBeenCalled();
    });
});

describe('visible refresh observation', () => {
    it('never overlaps reads and stops at terminal evidence without replaying a command', async () => {
        vi.useFakeTimers(); const gate = deferred(); const read = vi.fn().mockReturnValueOnce(gate.promise).mockResolvedValue({ observation: 'current', locally_owned: false, latest: { state: 'succeeded' } });
        const monitor = createRefreshMonitor({ read }); const initial = monitor.start();
        await vi.advanceTimersByTimeAsync(15000); expect(read).toHaveBeenCalledOnce();
        gate.resolve({ observation: 'current', locally_owned: true, latest: { state: 'running' } }); await initial;
        await vi.advanceTimersByTimeAsync(5000); expect(read).toHaveBeenCalledTimes(2);
        await vi.advanceTimersByTimeAsync(300000); expect(read).toHaveBeenCalledTimes(2);
    });
    it('stops on hidden, explicit cancellation, timeout and read error', async () => {
        vi.useFakeTimers(); let visible = true; const read = vi.fn().mockResolvedValue({ observation: 'current', locally_owned: true, latest: { state: 'running' } });
        const monitor = createRefreshMonitor({ read, visible: () => visible }); await monitor.start(); visible = false;
        await vi.advanceTimersByTimeAsync(5000); expect(read).toHaveBeenCalledOnce();
        visible = true; await monitor.start(); monitor.stop(); await vi.advanceTimersByTimeAsync(5000); expect(read).toHaveBeenCalledTimes(2);
        const timed = createRefreshMonitor({ read, now: () => Date.now() }); await timed.start(); await vi.advanceTimersByTimeAsync(310000); expect(read.mock.calls.length).toBeLessThanOrEqual(62);
        const error = vi.fn().mockRejectedValue(new Error('Private failure')); const failed = createRefreshMonitor({ read: error }); await failed.start(); await vi.advanceTimersByTimeAsync(300000); expect(error).toHaveBeenCalledOnce();
    });
});
