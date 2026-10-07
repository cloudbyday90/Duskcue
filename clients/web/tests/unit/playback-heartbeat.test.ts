import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createPlaybackHeartbeat } from '../../src/lib/playback/heartbeat.js';

function deferred() {
    let resolve: (value?: unknown) => void;
    let reject: (reason?: unknown) => void;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
}

const playing = () => ({ sessionId: 'session-a', positionMs: 905.9, isPlaying: true, isBuffering: false });

describe('per-store heartbeat lifecycle', () => {
    beforeEach(() => { vi.useFakeTimers(); });
    afterEach(() => { vi.unstubAllGlobals(); vi.clearAllTimers(); vi.useRealTimers(); });

    it('retains the fifteen-second first heartbeat and clears the owned timer', async () => {
        const send = vi.fn((_body) => Promise.resolve());
        const heartbeat = createPlaybackHeartbeat({ readState: playing, send });
        heartbeat.start();
        expect(vi.getTimerCount()).toBe(1);
        await vi.advanceTimersByTimeAsync(14999);
        expect(send).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(1);
        expect(send).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-a', position_ms: 905, state: 'playing', is_buffering: false });
        heartbeat.stopTimer();
        expect(vi.getTimerCount()).toBe(0);
        await vi.advanceTimersByTimeAsync(30000);
        expect(send).toHaveBeenCalledOnce();
    });

    it.each([
        { isPlaying: true, isBuffering: true, state: 'playing' },
        { isPlaying: false, isBuffering: false, state: 'paused' },
        { isPlaying: false, isBuffering: true, state: 'paused' },
    ])('preserves $state and buffering=$isBuffering in the transport body', async ({ isPlaying, isBuffering, state }) => {
        const send = vi.fn((_body) => Promise.resolve());
        const heartbeat = createPlaybackHeartbeat({ readState: () => ({ ...playing(), isPlaying, isBuffering }), send });
        await heartbeat.sendNow();
        expect(send).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-a', position_ms: 905, state, is_buffering: isBuffering });
    });

    it('captures an immutable body before transport runs and shares the pending session promise', async () => {
        const state = playing();
        const pending = deferred();
        const send = vi.fn((_body) => pending.promise);
        const heartbeat = createPlaybackHeartbeat({ readState: () => state, send });
        const first = heartbeat.sendNow();
        state.positionMs = 9200;
        state.isPlaying = false;
        state.isBuffering = true;
        expect(heartbeat.sendNow()).toBe(first);
        expect(heartbeat.drain('session-a')).toBe(first);
        await Promise.resolve();
        expect(send).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-a', position_ms: 905, state: 'playing', is_buffering: false });
        expect(Object.isFrozen(send.mock.calls[0][0])).toBe(true);
        pending.resolve();
        await expect(first).resolves.toBeUndefined();
    });

    it('deduplicates interval ticks while pending and reads a fresh position after settlement', async () => {
        const state = playing();
        const pending = deferred();
        const send = vi.fn((_body) => pending.promise);
        const heartbeat = createPlaybackHeartbeat({ readState: () => state, send });
        heartbeat.start();
        await vi.advanceTimersByTimeAsync(15000);
        state.positionMs = 6250.8;
        await vi.advanceTimersByTimeAsync(30000);
        expect(send).toHaveBeenCalledOnce();
        pending.resolve();
        await heartbeat.drain('session-a');
        send.mockResolvedValueOnce(undefined);
        await vi.advanceTimersByTimeAsync(15000);
        expect(send).toHaveBeenCalledTimes(2);
        expect(send.mock.calls[1][0].position_ms).toBe(6250);
        heartbeat.stopTimer();
    });

    it('keeps drain pending after timer cleanup until the actual transport settles', async () => {
        let state = playing();
        const pending = deferred();
        const send = vi.fn((_body) => pending.promise);
        const heartbeat = createPlaybackHeartbeat({ readState: () => state, send });
        heartbeat.start();
        const sent = heartbeat.sendNow();
        await Promise.resolve();
        heartbeat.stopTimer();
        state = { ...state, sessionId: null };
        let settled = false;
        const draining = heartbeat.drain('session-a').then(() => { settled = true; });
        await vi.advanceTimersByTimeAsync(30000);
        expect(settled).toBe(false);
        expect(send).toHaveBeenCalledOnce();
        pending.resolve({ position_ms: 905 });
        await draining;
        await sent;
        expect(settled).toBe(true);
        expect(vi.getTimerCount()).toBe(0);
    });

    it('drains a rejected transport quietly and permits the next deliberate send', async () => {
        const pending = deferred();
        const send = vi.fn((_body) => pending.promise);
        const heartbeat = createPlaybackHeartbeat({ readState: playing, send });
        const first = heartbeat.sendNow();
        const draining = heartbeat.drain('session-a');
        pending.reject(new Error('Heartbeat unavailable'));
        await expect(first).resolves.toBeUndefined();
        await expect(draining).resolves.toBeUndefined();
        send.mockResolvedValueOnce(undefined);
        await heartbeat.sendNow();
        expect(send).toHaveBeenCalledTimes(2);
    });

    it('settles synchronous transport failures without retaining a failed session entry', async () => {
        const send = vi.fn((_body) => { throw new Error('Transport failed before returning'); });
        const heartbeat = createPlaybackHeartbeat({ readState: playing, send });
        await expect(heartbeat.sendNow()).resolves.toBeUndefined();
        await expect(heartbeat.sendNow()).resolves.toBeUndefined();
        expect(send).toHaveBeenCalledTimes(2);
    });

    it('drains only the explicit old session while another session has an independent pending send', async () => {
        let state = playing();
        const first = deferred();
        const second = deferred();
        const send = vi.fn((body) => body.session_id === 'session-a' ? first.promise : second.promise);
        const heartbeat = createPlaybackHeartbeat({ readState: () => state, send });
        const old = heartbeat.sendNow();
        state = { ...state, sessionId: 'session-b', positionMs: 2010 };
        const current = heartbeat.sendNow();
        expect(current).not.toBe(old);
        let currentSettled = false;
        current.then(() => { currentSettled = true; });
        first.resolve();
        await heartbeat.drain('session-a');
        expect(currentSettled).toBe(false);
        expect(send).toHaveBeenCalledTimes(2);
        second.resolve();
        await heartbeat.drain('session-b');
        expect(currentSettled).toBe(true);
    });

    it('does not send without an active session and treats an empty drain as complete', async () => {
        const send = vi.fn((_body) => Promise.resolve());
        const heartbeat = createPlaybackHeartbeat({ readState: () => null, send });
        await expect(heartbeat.sendNow()).resolves.toBeUndefined();
        await expect(heartbeat.drain('missing-session')).resolves.toBeUndefined();
        expect(send).not.toHaveBeenCalled();
    });

    it('invalidates a queued interval callback when restarted or stopped', async () => {
        const callbacks: Array<() => void> = [];
        vi.stubGlobal('setInterval', (callback) => { callbacks.push(callback); return callbacks.length; });
        const clear = vi.fn();
        vi.stubGlobal('clearInterval', clear);
        const send = vi.fn((_body) => Promise.resolve());
        const heartbeat = createPlaybackHeartbeat({ readState: playing, send });
        heartbeat.start();
        heartbeat.start();
        callbacks[0]();
        await Promise.resolve();
        expect(send).not.toHaveBeenCalled();
        callbacks[1]();
        await heartbeat.drain('session-a');
        expect(send).toHaveBeenCalledOnce();
        heartbeat.stopTimer();
        callbacks[1]();
        await Promise.resolve();
        expect(send).toHaveBeenCalledOnce();
        expect(clear.mock.calls).toEqual([[1], [2]]);
    });

    it('keeps timer and pending transport ownership independent between store instances', async () => {
        const firstSend = vi.fn((_body) => Promise.resolve());
        const secondSend = vi.fn((_body) => Promise.resolve());
        const first = createPlaybackHeartbeat({ readState: playing, send: firstSend });
        const second = createPlaybackHeartbeat({ readState: playing, send: secondSend });
        first.start();
        second.start();
        await vi.advanceTimersByTimeAsync(15000);
        first.stopTimer();
        await vi.advanceTimersByTimeAsync(15000);
        expect(firstSend).toHaveBeenCalledOnce();
        expect(secondSend).toHaveBeenCalledTimes(2);
        second.stopTimer();
        expect(vi.getTimerCount()).toBe(0);
    });

    it.each([0, -1, Infinity, 2147483648])('rejects an unsafe timer duration %s', (intervalMs) => {
        expect(() => createPlaybackHeartbeat({ readState: playing, send: () => Promise.resolve(), intervalMs })).toThrow(RangeError);
    });
});
