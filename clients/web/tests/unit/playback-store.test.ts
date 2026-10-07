import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const api = vi.hoisted(() => ({
    startPlayback: vi.fn(async (_body: unknown) => ({ session_id: 'session-1', stream_decision: 'direct_play', transcode_session_id: null })),
    heartbeat: vi.fn(async (_body: unknown) => {}),
    stopPlayback: vi.fn(async (_body: unknown) => {}),
    seek: vi.fn(async (_body: unknown) => ({ stream_url: 'replacement-stream' })),
    getPlaybackInfo: vi.fn(async (_id: unknown) => ({ position_ms: 123, stream_decision: 'direct_play' })),
    streamFileUrl: vi.fn((id: string) => `/api/v1/stream/${id}`),
    transcodeManifestUrl: vi.fn((id: string) => `/api/v1/transcode/${id}/manifest.m3u8`),
}));

vi.mock('../../src/lib/api/playback.js', () => api);
import { createPlayerStore } from '../../src/lib/stores/player.js';
let player: ReturnType<typeof createPlayerStore>;

function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (value: Error) => void;
    const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
    return { promise, resolve, reject };
}

const item = { id: 'episode-1', type: 'episode', runtime_seconds: 900 };

beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    player = createPlayerStore();
    player.setContext({ serverOrigin: 'http://fixture.test', userId: 'user-a' });
});

afterEach(async () => {
    await player.stop().catch(() => {});
    player.reset();
    vi.clearAllTimers();
    vi.useRealTimers();
});

describe('integrated player session lifecycle', () => {
    it('retains the captured final position after failure and requires explicit release retry', async () => {
        await player.play(item, 'file-1');
        player.setPosition(9823.8);
        const failure = new Error('Stop unavailable');
        api.stopPlayback.mockRejectedValueOnce(failure);
        await expect(player.stop()).rejects.toBe(failure);
        expect(get(player)).toMatchObject({ sessionId: 'session-1', isPlaying: false, release: { phase: 'failed', error: failure } });
        player.setPosition(0);
        await expect(player.stop()).rejects.toBe(failure);
        expect(api.stopPlayback).toHaveBeenCalledOnce();
        await player.stop({ retry: true });
        expect(api.stopPlayback.mock.calls).toEqual([[{ session_id: 'session-1', position_ms: 9823 }], [{ session_id: 'session-1', position_ms: 9823 }]]);
        expect(get(player).sessionId).toBeNull();
    });

    it('preserves same-account cleanup through forced reset and blocks a new visible title until explicit retry', async () => {
        await player.play(item, 'file-1');
        player.setPosition(4200);
        const failure = new Error('Stop unavailable');
        api.stopPlayback.mockRejectedValueOnce(failure);
        await expect(player.stop()).rejects.toBe(failure);
        player.reset({ preserveRelease: true });
        player.setContext({ serverOrigin: 'http://fixture.test/', userId: 'user-a' });
        expect(get(player)).toMatchObject({ sessionId: null, release: { phase: 'failed' } });
        await expect(player.play({ ...item, id: 'episode-2' }, 'file-2')).rejects.toBe(failure);
        expect(api.startPlayback).toHaveBeenCalledOnce();
        expect(api.stopPlayback).toHaveBeenCalledOnce();
        await player.stop({ retry: true });
        expect(api.stopPlayback).toHaveBeenLastCalledWith({ session_id: 'session-1', position_ms: 4200 });
        api.startPlayback.mockResolvedValueOnce({ session_id: 'session-2', stream_decision: 'direct_play', transcode_session_id: null });
        await player.play({ ...item, id: 'episode-2' }, 'file-2');
        expect(get(player).sessionId).toBe('session-2');
    });

    it.each([
        { serverOrigin: 'http://other.test', userId: 'user-a' },
        { serverOrigin: 'http://fixture.test', userId: 'user-b' },
        { serverOrigin: 'http://fixture.test', userId: null },
    ])('discards old cleanup without sending its ID after context changes to $serverOrigin / $userId', async (context) => {
        await player.play(item, 'file-1');
        api.stopPlayback.mockRejectedValueOnce(new Error('Stop unavailable'));
        await expect(player.stop()).rejects.toThrow('Stop unavailable');
        player.setContext(context);
        await player.stop({ retry: true });
        expect(api.stopPlayback).toHaveBeenCalledOnce();
        expect(get(player)).toMatchObject({ sessionId: null, release: { phase: 'idle', error: null } });
    });

    it('does not release a late allocated session through a changed account or server', async () => {
        const pending = deferred<{ session_id: string; stream_decision: string; transcode_session_id: null }>();
        api.startPlayback.mockReturnValueOnce(pending.promise);
        const starting = player.play(item, 'file-1');
        await vi.waitFor(() => expect(api.startPlayback).toHaveBeenCalledOnce());
        player.setContext({ serverOrigin: 'http://other.test', userId: 'user-b' });
        pending.resolve({ session_id: 'old-account-session', stream_decision: 'direct_play', transcode_session_id: null });
        expect(await starting).toBeNull();
        expect(api.stopPlayback).not.toHaveBeenCalled();
        expect(get(player).sessionId).toBeNull();
    });

    it('drains the actual pending heartbeat before all exit callers complete', async () => {
        await player.play(item, 'file-1');
        const pending = deferred<void>();
        const order: string[] = [];
        api.heartbeat.mockImplementationOnce(async () => { await pending.promise; order.push('heartbeat'); });
        api.stopPlayback.mockImplementationOnce(async () => { order.push('stop'); });
        const heartbeat = player.sendHeartbeatNow();
        player.setPosition(9000);
        const firstExit = player.stop();
        const secondExit = player.stop();
        await Promise.resolve();
        expect(api.stopPlayback).not.toHaveBeenCalled();
        pending.resolve();
        await Promise.all([heartbeat, firstExit, secondExit]);
        expect(order).toEqual(['heartbeat', 'stop']);
        expect(api.stopPlayback).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 9000 });
        expect(vi.getTimerCount()).toBe(0);
    });
    it('releases an aborted owned start without advancing watch history', async () => {
        const pending = deferred<{ session_id: string; stream_decision: string; transcode_session_id: null }>();
        api.startPlayback.mockReturnValueOnce(pending.promise);
        const controller = new AbortController();
        const starting = player.play(item, 'file-1', { signal: controller.signal });
        await vi.waitFor(() => expect(api.startPlayback).toHaveBeenCalledOnce());
        controller.abort();
        pending.resolve({ session_id: 'aborted-session', stream_decision: 'direct_play', transcode_session_id: null });
        expect(await starting).toBeNull();
        expect(api.stopPlayback).toHaveBeenCalledExactlyOnceWith({ session_id: 'aborted-session', position_ms: 0, cancelled_before_start: true });
        expect(get(player).loading).toBe(false);
        expect(get(player).sessionId).toBeNull();
    });
    it('releases a late start after closing and never adopts it or starts a heartbeat', async () => {
        const pending = deferred<{ session_id: string; stream_decision: string; transcode_session_id: null }>();
        api.startPlayback.mockReturnValueOnce(pending.promise);
        const play = player.play(item, 'file-1', { startPositionMs: 456 });
        await vi.waitFor(() => expect(api.startPlayback).toHaveBeenCalledOnce());
        const stopping = player.stop();
        pending.resolve({ session_id: 'late-session', stream_decision: 'direct_play', transcode_session_id: null });
        await stopping;
        expect(await play).toBeNull();
        expect(get(player).sessionId).toBeNull();
        expect(api.stopPlayback).toHaveBeenCalledExactlyOnceWith({ session_id: 'late-session', position_ms: 0, cancelled_before_start: true });
        vi.advanceTimersByTime(30000);
        expect(api.heartbeat).not.toHaveBeenCalled();
    });

    it('stops the actual session once with its current position across repeated exits', async () => {
        await player.play(item, 'file-1');
        player.setPosition(9823.8);
        await Promise.all([player.stop(), player.stop()]);
        expect(api.stopPlayback).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 9823 });
        expect(get(player).sessionId).toBeNull();
    });

    it('forwards real track indices and device quality when starting playback', async () => {
        await player.play(item, 'file-1', { audioStreamIndex: 0, subtitleStreamIndex: 3, qualityMode: 'manual', maxBitrate: 4000000, forceTranscode: true });
        expect(api.startPlayback).toHaveBeenCalledWith(expect.objectContaining({
            audio_stream_index: 0, subtitle_stream_index: 3, quality_mode: 'manual', max_streaming_bitrate: 4000000, force_transcode: true,
        }));
    });

    it('keeps a newer session intact when the preceding stop resolves late', async () => {
        await player.play(item, 'file-1');
        const pending = deferred<void>();
        api.stopPlayback.mockReturnValueOnce(pending.promise);
        const stopping = player.stop();
        api.startPlayback.mockResolvedValueOnce({ session_id: 'session-2', stream_decision: 'direct_play', transcode_session_id: null });
        const starting = player.play({ ...item, id: 'episode-2' }, 'file-2');
        await vi.waitFor(() => expect(api.stopPlayback).toHaveBeenCalledOnce());
        expect(api.startPlayback).toHaveBeenCalledOnce();
        pending.resolve();
        await starting;
        await stopping;
        expect(get(player).sessionId).toBe('session-2');
    });

    it('ignores a late seek replacement after playback exits', async () => {
        await player.play(item, 'file-1');
        const pending = deferred<{ stream_url: string }>();
        api.seek.mockReturnValueOnce(pending.promise);
        const seeking = player.seek(9000);
        await player.stop();
        pending.resolve({ stream_url: 'late-stream' });
        expect(await seeking).toBeNull();
        expect(get(player).streamUrl).toBeNull();
    });
});
