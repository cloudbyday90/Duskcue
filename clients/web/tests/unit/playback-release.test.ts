import { describe, expect, it, vi } from 'vitest';
import { createPlaybackRelease, playbackContextKey } from '../../src/lib/playback/release.js';
import { createPlaybackSessionGuard } from '../../src/lib/playback/session.js';

function deferred() {
    let resolve: (value?: unknown) => void;
    const promise = new Promise((yes) => { resolve = yes; });
    return { promise, resolve };
}

const key = playbackContextKey({ serverOrigin: 'http://fixture.test', userId: 'user-a' });
const snapshot = () => ({ sessionId: 'session-a', positionMs: 9123.9, contextKey: key });

describe('explicit account-owned playback release', () => {
    it('shares pending release and waits for the actual heartbeat before sending immutable final data', async () => {
        const heartbeat = deferred();
        const send = vi.fn((_record, _isCurrent) => Promise.resolve());
        const release = createPlaybackRelease({ readContext: () => key, drain: () => heartbeat.promise, send });
        const state = snapshot();
        const first = release.request(state);
        state.positionMs = 0;
        expect(release.request(state)).toBe(first);
        await Promise.resolve();
        expect(send).not.toHaveBeenCalled();
        heartbeat.resolve();
        await first;
        expect(send.mock.calls[0][0]).toMatchObject({ sessionId: 'session-a', positionMs: 9123, contextKey: key, cancelledBeforeStart: false });
        expect(Object.isFrozen(send.mock.calls[0][0])).toBe(true);
        expect(release.getState()).toEqual({ phase: 'idle', error: null });
    });

    it('keeps failed final data for explicit retry without automatically issuing another stop', async () => {
        const failure = new Error('Stop unavailable');
        const send = vi.fn((_record, _isCurrent) => Promise.resolve()).mockRejectedValueOnce(failure);
        const release = createPlaybackRelease({ readContext: () => key, drain: () => Promise.resolve(), send });
        await expect(release.request(snapshot())).rejects.toBe(failure);
        expect(release.getState()).toEqual({ phase: 'failed', error: failure });
        await expect(release.request({ ...snapshot(), positionMs: 0 })).rejects.toBe(failure);
        expect(send).toHaveBeenCalledOnce();
        await release.request(null, { retry: true });
        expect(send).toHaveBeenCalledTimes(2);
        expect(send.mock.calls[1][0].positionMs).toBe(9123);
        expect(Object.keys(release.getState()).sort()).toEqual(['error', 'phase']);
    });

    it('evicts a rejected guard entry and replays the same final request after a lost acknowledgement', async () => {
        const response = { session_id: 'session-a', play_count: 1, is_watched: false };
        let committed = false;
        let commits = 0;
        const transport = vi.fn(async (_body) => {
            if (!committed) {
                committed = true;
                commits += 1;
                throw new Error('Acknowledgement lost after commit');
            }
            return response;
        });
        const guard = createPlaybackSessionGuard(transport);
        const release = createPlaybackRelease({
            readContext: () => key, drain: () => Promise.resolve(),
            send: (record, isCurrent) => guard.stop(record.sessionId, record.positionMs, record.cancelledBeforeStart, { contextKey: record.contextKey, isCurrent }),
        });
        await expect(release.request(snapshot())).rejects.toThrow('Acknowledgement lost');
        expect(transport).toHaveBeenCalledOnce();
        await expect(release.request(null, { retry: true })).resolves.toEqual(response);
        expect(commits).toBe(1);
        expect(transport.mock.calls).toEqual([[{ session_id: 'session-a', position_ms: 9123 }], [{ session_id: 'session-a', position_ms: 9123 }]]);
        await guard.stop('session-a', 0, false, { contextKey: key });
        expect(transport).toHaveBeenCalledTimes(2);
    });

    it('invalidates queued old-context cleanup before transport and clears its private retry record', async () => {
        let context = key;
        const heartbeat = deferred();
        const send = vi.fn((_record, _isCurrent) => Promise.resolve());
        const release = createPlaybackRelease({ readContext: () => context, drain: () => heartbeat.promise, send });
        const pending = release.request(snapshot());
        const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
        await Promise.resolve();
        context = playbackContextKey({ serverOrigin: 'http://other.test', userId: 'user-b' });
        release.invalidate();
        heartbeat.resolve();
        await rejected;
        expect(send).not.toHaveBeenCalled();
        expect(release.hasPending()).toBe(false);
        expect(release.getState()).toEqual({ phase: 'idle', error: null });
    });

    it('rejects a mismatched captured account context before any stop is sent', async () => {
        const send = vi.fn((_record, _isCurrent) => Promise.resolve());
        const release = createPlaybackRelease({ readContext: () => 'different-context', drain: () => Promise.resolve(), send });
        await expect(release.request(snapshot())).rejects.toMatchObject({ name: 'AbortError' });
        expect(send).not.toHaveBeenCalled();
    });
});
