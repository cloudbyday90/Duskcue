// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { afterEach, describe, expect, it, vi } from 'vitest';
import { webcrypto } from 'node:crypto';
import { capability, PROTOCOL, scope } from '../../src/lib/playback/commands/schema.js';
import { createServerClock } from '../../src/lib/playback/commands/clock.js';
import { createJournal } from '../../src/lib/playback/commands/journal.js';
import { createCommandController } from '../../src/lib/playback/commands/controller.js';
import { createCheckpoints } from '../../src/lib/playback/commands/checkpoints.js';
import { createLease } from '../../src/lib/playback/commands/lease.js';
import { createStatusReader } from '../../src/lib/playback/commands/status-reader.js';
import { createCommandAdapter } from '../../src/lib/playback/commands/adapter.js';

const id = (tail = 1) => `019a2a01-0000-7000-8000-${tail.toString().padStart(12, '0')}`;
const origin = 'https://media.example';
const observed = '2026-10-28T10:00:00.000Z';
const context = { user_id: id(2), session_id: id(3), profile_id: id(4), credential_revision: '1', profile_context_revision: '1',
    context_id: `sc1.${'a'.repeat(43)}`, transport: 'cookie', observed_at: observed, session_expires_at: '2026-10-28T11:00:00.000Z' };
function journalFixture() {
    const values = new Map();
    const storage = { get length() { return values.size; }, clear: () => { values.clear(); }, key: (index) => [...values.keys()][index] ?? null,
        getItem: (key) => values.get(key) ?? null, setItem: (key, value) => { values.set(key, value); }, removeItem: (key) => { values.delete(key); } };
    return { journal: createJournal({ storage: () => storage }), values };
}
function fixture({ lostStart = false, lostWithdraw = false, pending = false } = {}) {
    let now = 0;
    const { journal, values } = journalFixture();
    const clock = createServerClock({ now: () => now, wallNow: () => now, crypto: webcrypto });
    const calls = [];
    let start;
    const transport = { context: async () => ({ context, scope: scope(origin, context), began: now }), current: () => true,
        call: vi.fn(async (method, path, _scope, options: any = {}): Promise<any> => {
            calls.push({ method, path, body: options.body });
            start ||= options.body?.request_id;
            const base = { protocol: PROTOCOL, request_id: start, observed_at: observed };
            if (method === 'POST' && path === '/api/v1/playback-commands' && lostStart) throw new TypeError('lost response');
            if (path.endsWith('/withdraw') && lostWithdraw) throw new TypeError('lost withdrawal');
            if (path.endsWith('/withdraw') || path.endsWith('/settlement')) return { status: method === 'POST' ? 202 : 200, began: now,
                location: `/api/v1/playback-commands/${start}/settlement`, body: { ...base, status_url: `/api/v1/playback-commands/${start}/settlement`, status: 'settled', admission_fenced: true, preparation_complete: true, parent_released: null } };
            if (path.endsWith('/checkpoint')) return { status: 200, began: now, body: { ...base, sequence: 0, output_revision: null, position_ms: null, playback_state: 'playing' } };
            return { status: method === 'POST' ? 202 : 200, began: now, location: `/api/v1/playback-commands/${start}`, body: {
                ...base, status_url: `/api/v1/playback-commands/${start}`, status: pending ? 'preparing' : 'ready',
                lease_deadline: '2026-10-28T10:01:00.000Z', stream_url: pending ? null : `/api/v1/playback-commands/${start}/outputs/${id(8)}/file`,
                ready: pending ? null : { output_revision: id(8), session_id: id(9), media_item_id: id(10), media_file_id: id(11), delivery_mode: 'direct_play', position_ms: 0, stream_offset_ms: null },
            } };
        }) };
    const controller = createCommandController({ transport, journal, clock, now: () => now, wait: async (ms) => { now += ms; } });
    return { controller, calls, transport, values, journal };
}
afterEach(() => vi.useRealTimers());

describe('retained VOD client commands', () => {
    it('preserves the latest explicit play intent across retired media events and replacement output', async () => {
        let state = { streamUrl: null, isPlaying: false };
        let current = { phase: 'idle', record: null, receipt: null, streamUrl: null };
        let changed;
        const ready = { session_id: id(9), output_revision: id(8), media_file_id: id(11), delivery_mode: 'direct_play', position_ms: 0 };
        const publish = (phase, url = null) => {
            current = { ...current, phase, streamUrl: url, receipt: { ready }, record: { start: id() } };
            changed(current);
        };
        const adapter = createCommandAdapter({ read: () => state, update: (change) => { state = change(state); }, revision: () => {},
            createController: ({ onChange }) => {
                changed = onChange;
                return { restore: async () => false, active: () => !!current.record, getState: () => current,
                    start: async () => publish('ready', 'first'), dispose: () => {} } as ReturnType<typeof createCommandController>;
            } });
        await adapter.play({ id: id(10) }, id(11), { playing: true }, () => true);
        adapter.setPlaying(false);
        publish('changing'); publish('ready', 'second');
        expect(state.isPlaying).toBe(false);
        expect(adapter.setPlaying(true, true)).toBe(false);
        expect(adapter.playing()).toBe(false);
        adapter.setPlaying(true);
        publish('changing');
        adapter.setPlaying(false, true);
        publish('ready', 'third');
        expect(state.isPlaying).toBe(true);
        expect(adapter.playing()).toBe(true);
        publish('changing'); adapter.setPlaying(false); publish('ready', 'fourth');
        expect(state.isPlaying).toBe(false);
        adapter.dispose();
    });
    it('requires a real output-bound checkpoint before automatically recovering an offered worker failure', async () => {
        const f = fixture();
        await f.controller.start({});
        const original = f.transport.call;
        const start = f.controller.getState().record.start;
        const recoveries = [];
        let recovered = false;
        let operation;
        f.transport.call = vi.fn(async (method, path, captured, options = {}) => {
            const reply = await original(method, path, captured, options);
            if (path.endsWith('/checkpoint') && method === 'POST') return { status: 200, began: 0, body: { ...options.body, request_id: start } };
            if (path.endsWith('/configuration')) {
                if (method === 'GET') return { status: 200, began: 0, etag: `"vod-config-${'b'.repeat(64)}"`, body: { protocol: PROTOCOL, request_id: start,
                    revision: '2', state: 'unavailable', output_revision: id(8), pending_operation: null, worker_recovery: true } };
                recoveries.push(options.body); operation = options.body.operation_id;
                return { status: 202, began: 0, location: `/api/v1/playback-commands/${start}/operations/${operation}`, body: {
                    protocol: PROTOCOL, request_id: start, operation_id: operation, status: 'accepted', cleanup_complete: false,
                    status_url: `/api/v1/playback-commands/${start}/operations/${operation}` } };
            }
            if (path.includes('/operations/')) {
                recovered = true;
                return { status: 200, began: 0, body: { protocol: PROTOCOL, request_id: start, operation_id: operation, status: 'completed', cleanup_complete: true,
                    status_url: `/api/v1/playback-commands/${start}/operations/${operation}` } };
            }
            if (path === `/api/v1/playback-commands/${start}` && method === 'GET') {
                if (!recovered) return { ...reply, body: { ...reply.body, status: 'unavailable', ready: null, stream_url: null } };
                return { ...reply, body: { ...reply.body, ready: { ...reply.body.ready, output_revision: id(12) }, stream_url: `/api/v1/playback-commands/${start}/outputs/${id(12)}/file` } };
            }
            return reply;
        });
        await f.controller.checkpoint();
        expect(recoveries).toHaveLength(0);
        f.controller.observe({ output: id(8), position: 1000, state: 'paused', attached: true });
        await f.controller.checkpoint();
        expect(recoveries).toHaveLength(1);
        expect(recoveries[0]).toMatchObject({ expected_output: id(8), action: { kind: 'recover', checkpoint_sequence: 1, evidence: { kind: 'worker' } } });
        expect(f.controller.getState()).toMatchObject({ phase: 'ready', record: { output: id(12) } });
        expect(f.calls.filter((call) => call.path === '/api/v1/playback-commands')).toHaveLength(1);
        f.controller.dispose();
    });
    it('reconciles a checkpoint that never committed without replaying its observed position', async () => {
        const send = vi.fn().mockRejectedValueOnce(new Error('lost before commit')).mockImplementation(async (body) => body);
        const read = async () => ({ protocol: PROTOCOL, output_revision: null, sequence: 0, position_ms: null, playback_state: 'playing' });
        const checkpoint = createCheckpoints({ send, read, current: () => true, onUncertain: () => {} });
        checkpoint.observe({ output: id(8), position: 1000, state: 'playing', attached: true });
        await expect(checkpoint.flush(true)).rejects.toThrow();
        await checkpoint.reconcile();
        expect(checkpoint.confirmed().position_ms).toBeNull();
        checkpoint.observe({ output: id(8), position: 1500, state: 'paused', attached: true });
        await checkpoint.flush(true);
        expect(send.mock.calls[1][0]).toMatchObject({ sequence: 2, position_ms: 1500 });
    });
    it('separates replacement checkpoint sequences from late retired-output acknowledgements', async () => {
        let finish;
        let output = id(8);
        const send = vi.fn().mockImplementationOnce((body) => new Promise((resolve) => { finish = () => resolve(body); }))
            .mockImplementation(async (body) => body);
        const read = vi.fn(async () => ({ protocol: PROTOCOL, output_revision: null, sequence: 0, position_ms: null, playback_state: 'paused' }));
        const checkpoint = createCheckpoints({ send, read, current: (id) => id === output, onUncertain: () => {} });
        checkpoint.observe({ output, position: 1000, state: 'playing', attached: true });
        const old = checkpoint.flush(true);
        await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
        output = id(12); checkpoint.reset();
        const reconciled = checkpoint.reconcile();
        finish(); await old; await reconciled;
        checkpoint.observe({ output, position: 4000, state: 'paused', attached: true });
        await checkpoint.flush(true);
        expect(send.mock.calls[1][0]).toMatchObject({ sequence: 1, output_revision: id(12), position_ms: 4000 });
    });
    it('uses the server current revision for explicit correction after failed replacement', async () => {
        const f = fixture();
        await f.controller.start({});
        const original = f.transport.call;
        let output = id(8);
        let operation;
        const changes = [];
        f.transport.call = vi.fn(async (method, path, captured, options = {}) => {
            const start = f.controller.getState().record.start;
            if (path.endsWith('/configuration')) {
                if (method === 'GET') return { status: 200, began: 0, etag: `"vod-config-${'a'.repeat(64)}"`, body: { protocol: PROTOCOL, request_id: start,
                    revision: '2', state: output === id(8) ? 'ready' : 'unavailable', output_revision: output, pending_operation: null, worker_recovery: false } };
                changes.push(options.body); operation = options.body.operation_id; output = id(12);
                return { status: 202, began: 0, location: `/api/v1/playback-commands/${start}/operations/${operation}`, body: {
                    protocol: PROTOCOL, request_id: start, operation_id: operation, status: 'accepted', cleanup_complete: false,
                    status_url: `/api/v1/playback-commands/${start}/operations/${operation}` } };
            }
            if (path.includes('/operations/')) return { status: 200, began: 0, body: { protocol: PROTOCOL, request_id: start, operation_id: operation,
                status: 'failed', cleanup_complete: true, status_url: `/api/v1/playback-commands/${start}/operations/${operation}` } };
            return original(method, path, captured, options);
        });
        await f.controller.change({ kind: 'seek', position_ms: 2000 });
        await f.controller.change({ kind: 'reconfigure', position_ms: 2000, selection: {} });
        expect(changes.map((value) => value.expected_output)).toEqual([id(8), id(12)]);
        expect(f.calls.filter((call) => call.path === '/api/v1/playback-commands')).toHaveLength(1);
        f.controller.dispose();
    });
    it('detaches media on a credential boundary even when journal removal fails', async () => {
        const f = fixture();
        await f.controller.start({});
        expect(f.controller.getState().streamUrl).not.toBeNull();
        f.journal.clear = () => { throw new Error('storage refused'); };
        expect(() => f.controller.invalidate()).not.toThrow();
        expect(f.controller.getState()).toMatchObject({ phase: 'idle', streamUrl: null, record: null });
        f.controller.dispose();
    });
    it('keeps profile cleanup identity and detaches media when a journal update fails', async () => {
        const f = fixture();
        await f.controller.start({});
        const original = f.controller.getState().record;
        f.journal.write = () => { throw new Error('storage refused'); };
        expect(() => f.controller.invalidate({ ...original.scope, profile: id(99), revision: '2' })).not.toThrow();
        expect(f.controller.getState()).toMatchObject({ phase: 'status_required', streamUrl: null, cleanupOnly: true, record: { start: original.start, phase: 'cleanup' } });
        f.controller.dispose();
    });
    it('spaces distinct status resources and stops observation at its deadline', async () => {
        let now = 0;
        const starts = [];
        const transport = { call: vi.fn(async () => { starts.push(now); return {}; }) };
        const statuses = createStatusReader({ transport, owns: () => true, now: () => now, wait: async (ms) => { now += ms; } });
        const record = { start: id(), scope: scope(origin, context) };
        await statuses.read(record, 1, '/operations/' + id(7), false, 2000);
        await statuses.read(record, 1, '', false, 2000);
        expect(await statuses.read(record, 1, '/settlement', true, 2000)).toBeNull();
        expect(starts).toEqual([0, 1000]);
        expect(now).toBe(2000);
    });
    it('requires the complete positive capability; absence is the only legacy selection', () => {
        expect(capability(undefined)).toBeNull();
        expect(() => capability({ protocol: PROTOCOL, features: [] })).toThrow();
        expect(capability({ protocol: PROTOCOL, features: ['start-settlement', 'conditional-controls', 'checkpoints', 'lease-observation'] })).toBe(PROTOCOL);
    });
    it('uses CSPRNG UUIDv7 and refuses stale database time', () => {
        let now = 0;
        const clock = createServerClock({ now: () => now, wallNow: () => now, crypto: webcrypto });
        expect(() => clock.id()).toThrow();
        clock.observe(observed);
        const generated = clock.id();
        expect(generated[14]).toBe('7');
        expect(BigInt(`0x${generated.slice(0, 13).replaceAll('-', '')}`)).toBe(BigInt(Date.parse(observed)));
        now = 30_001;
        expect(() => clock.id()).toThrow();
    });
    it('retains exactly one minimal journal before an uncertain Start and conclusively withdraws it', async () => {
        const f = fixture({ lostStart: true });
        await expect(f.controller.start({ private_title: 'never persist' })).rejects.toThrow();
        expect(f.calls.filter((call) => call.method === 'POST')).toHaveLength(1);
        const saved = [...f.values.values()][0];
        expect(saved).not.toContain('private_title');
        expect(f.controller.getState().phase).toBe('uncertain');
        await expect(f.controller.start({})).rejects.toThrow();
        expect(await f.controller.close()).toBe(true);
        expect(f.journal.read()).toBeNull();
        expect(f.calls.filter((call) => call.path === '/api/v1/playback-commands')).toHaveLength(1);
        f.controller.dispose();
    });
    it('does not repeat a lost withdrawal while checking settlement', async () => {
        const f = fixture({ lostStart: true, lostWithdraw: true });
        await expect(f.controller.start({})).rejects.toThrow();
        await expect(f.controller.close()).rejects.toThrow();
        expect(await f.controller.check()).toBe(true);
        expect(f.calls.filter((call) => call.path.endsWith('/withdraw'))).toHaveLength(1);
        expect(f.journal.read()).toBeNull();
        f.controller.dispose();
    });
    it('limits accepted-status observation without replaying mutation', async () => {
        const f = fixture({ pending: true });
        await expect(f.controller.start({})).rejects.toThrow();
        expect(f.calls.filter((call) => call.method === 'GET')).toHaveLength(60);
        expect(f.calls.filter((call) => call.method === 'POST')).toHaveLength(1);
        expect(f.journal.read()).not.toBeNull();
        f.controller.dispose();
    });
    it('refuses Start when durable identity cannot be written', async () => {
        const f = fixture();
        const controller = createCommandController({ transport: f.transport, journal: { ...f.journal, write: () => { throw new Error('quota'); } } });
        await expect(controller.start({})).rejects.toThrow();
        expect(f.calls).toHaveLength(0);
        controller.dispose(); f.controller.dispose();
    });
    it('renews while paused and stops automatic dispatch after a lost renewal', async () => {
        vi.useFakeTimers();
        const renew = vi.fn().mockRejectedValue(new Error('lost'));
        const onUncertain = vi.fn();
        const lease = createLease({ remaining: () => 60_000, renew, onUncertain, now: () => Date.now() });
        lease.start();
        await vi.advanceTimersByTimeAsync(15_000);
        await vi.advanceTimersByTimeAsync(90_000);
        expect(renew).toHaveBeenCalledTimes(1);
        expect(onUncertain).toHaveBeenCalledWith('uncertain');
        lease.stop();
    });
    it('ignores requested or detached positions and reconciles lost checkpoint sequence', async () => {
        const send = vi.fn().mockRejectedValue(new Error('lost'));
        const read = vi.fn().mockResolvedValue({ protocol: PROTOCOL, output_revision: id(8), sequence: 1, position_ms: 1234, playback_state: 'paused' });
        const checkpoint = createCheckpoints({ read, send, current: () => true, onUncertain: () => {} });
        checkpoint.observe({ output: id(8), position: 9999, attached: false, state: 'playing' });
        await checkpoint.flush(true);
        expect(send).not.toHaveBeenCalled();
        checkpoint.observe({ output: id(8), position: 1234, attached: true, state: 'paused' });
        await expect(checkpoint.flush(true)).rejects.toThrow();
        await checkpoint.flush(true);
        expect(send).toHaveBeenCalledTimes(1);
        expect((await checkpoint.reconcile()).sequence).toBe(1);
    });
    it('Close fences a Start still waiting for authoritative context before any POST', async () => {
        const f = fixture();
        let finish;
        f.transport.context = () => new Promise((resolve) => { finish = resolve; });
        const start = f.controller.start({});
        await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
        expect(await f.controller.close()).toBe(true);
        finish({ context, scope: scope(origin, context), began: 0 });
        await expect(start).rejects.toThrow();
        expect(f.calls).toHaveLength(0);
        expect(f.journal.read()).toBeNull();
        f.controller.dispose();
    });
    it('late settlement cannot revive a cleared scope', async () => {
        const f = fixture({ lostStart: true });
        await expect(f.controller.start({})).rejects.toThrow();
        const send = f.transport.call;
        let finish;
        f.transport.call = vi.fn(async (...args) => {
            const value = await send(...args);
            return args[1].endsWith('/settlement') ? new Promise((resolve) => { finish = () => resolve(value); }) : value;
        });
        const closing = f.controller.close();
        await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
        f.controller.invalidate();
        finish();
        await expect(closing).rejects.toThrow();
        expect(f.controller.getState().phase).toBe('idle');
        expect(f.journal.read()).toBeNull();
        f.controller.dispose();
    });
    it('an old renewal failure cannot stop a freshly reconciled lease', async () => {
        vi.useFakeTimers();
        let reject;
        const renew = vi.fn().mockImplementationOnce(() => new Promise((_, refuse) => { reject = refuse; })).mockResolvedValue(undefined);
        const onUncertain = vi.fn();
        const lease = createLease({ remaining: () => 60_000, renew, onUncertain, now: () => Date.now() });
        lease.start(); await vi.advanceTimersByTimeAsync(15_000);
        lease.stop(); lease.start(); reject(new Error('old response'));
        await vi.advanceTimersByTimeAsync(15_000);
        expect(renew).toHaveBeenCalledTimes(2);
        expect(onUncertain).not.toHaveBeenCalled();
        lease.stop();
    });
    it('coalesces concurrent final checkpoint drains to one additional observation', async () => {
        let finish;
        const send = vi.fn().mockImplementationOnce((body) => new Promise((resolve) => { finish = () => resolve({ ...body, request_id: id() }); }))
            .mockImplementation(async (body) => ({ ...body, request_id: id() }));
        const checkpoint = createCheckpoints({ read: async () => null, send, current: () => true, onUncertain: () => {} });
        checkpoint.observe({ output: id(8), position: 1000, attached: true, state: 'playing' });
        const first = checkpoint.flush(true);
        await vi.waitFor(() => expect(finish).toBeTypeOf('function'));
        checkpoint.observe({ output: id(8), position: 1500, attached: true, state: 'paused' });
        const a = checkpoint.flush(true); const b = checkpoint.flush(true);
        finish(); await Promise.all([first, a, b]);
        expect(send).toHaveBeenCalledTimes(2);
        expect(checkpoint.confirmed().position_ms).toBe(1500);
    });
});
