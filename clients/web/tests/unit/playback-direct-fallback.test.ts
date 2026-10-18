import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const api = vi.hoisted(() => ({
    startPlayback: vi.fn(async (_body: unknown): Promise<any> => ({})),
    heartbeat: vi.fn(async (_body: unknown): Promise<any> => undefined),
    stopPlayback: vi.fn(async (_body: unknown) => undefined),
    seek: vi.fn(async (_body: unknown): Promise<any> => ({})),
    getPlaybackInfo: vi.fn(async (_id: unknown): Promise<any> => ({})),
    streamFileUrl: (id: string) => `/api/v1/stream/${id}`,
    transcodeManifestUrl: (id: string) => `/api/v1/transcode/${id}/manifest.m3u8`,
}));
vi.mock('../../src/lib/api/playback.js', () => api);
import { createPlayerStore } from '../../src/lib/stores/player.js';

const revision = '00000000-0000-7000-8000-000000000001';
const nextRevision = '00000000-0000-7000-8000-000000000002';
const descriptor = {
    video: { content_type: 'video/mp4; codecs="avc1.640028"', width: 320, height: 180, bitrate: 500000, framerate: 30 },
    audio: { content_type: 'audio/mp4; codecs="mp4a.40.2"', channels: '2', bitrate: 128000, samplerate: 48000 },
};
const direct = () => ({ session_id: 'session-1', stream_decision: 'direct_play', direct_source_revision: revision, direct_fallback: descriptor });
const recovered = () => ({ session_id: 'session-1', stream_decision: 'transcode', transcode_session_id: 'worker-1', stream_offset_ms: 4500, position_ms: 4500,
    direct_source_revision: null, direct_fallback: null, recovery: { status: 'recovered', attempts: 1, reason: 'direct_decode', skipped_ms: 0, quality_reduced: false } });
const failure = () => ({ positionMs: 4500, sourceUrl: '/api/v1/stream/file-1', sourceMode: 'direct', mediaErrorCode: 3, recoveryRequested: true });
function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((accept) => { resolve = accept; });
    return { promise, resolve };
}
const flush = async () => { for (let index = 0; index < 20; index += 1) await Promise.resolve(); };
const createProbe = () => vi.fn(async (_descriptor: unknown, _options: { signal?: AbortSignal } = {}): Promise<string> => 'supported');
let player: ReturnType<typeof createPlayerStore>;
let probe: ReturnType<typeof createProbe>;

beforeEach(() => {
    vi.useFakeTimers();
    vi.resetAllMocks();
    api.startPlayback.mockResolvedValue(direct());
    api.heartbeat.mockResolvedValue(recovered());
    api.seek.mockResolvedValue({ position_ms: 9000, direct_source_revision: nextRevision });
    probe = createProbe();
    player = createPlayerStore({ probeFallback: probe });
    player.setContext({ serverOrigin: 'http://fixture.test', userId: 'user-a' });
});
afterEach(() => { player.destroy(); player.reset(); vi.clearAllTimers(); vi.useRealTimers(); });
const start = () => player.play({ id: 'item-1', runtime_seconds: 120 }, 'file-1', { audioStreamIndex: 2 });

describe('revision-owned direct native decode fallback', () => {
    it('probes admitted output then atomically adopts a worker under the existing play session and selected tracks', async () => {
        await start();
        expect(await player.reportSourceFailure(failure())).toBe(true);
        expect(probe).toHaveBeenCalledExactlyOnceWith(descriptor, expect.objectContaining({ signal: expect.any(AbortSignal) }));
        expect(api.heartbeat).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 4500, state: 'paused', is_buffering: true,
            direct_source_revision: revision, direct_fallback_supported: true, recovery_requested: true });
        expect(get(player)).toMatchObject({ sessionId: 'session-1', streamDecision: 'transcode', transcodeSessionId: 'worker-1',
            streamUrl: '/api/v1/transcode/worker-1/manifest.m3u8', streamOffsetMs: 4500, positionMs: 4500, isPlaying: true,
            directSourceRevision: null, directFallback: null, directFallbackSupported: false, recovery: { status: 'recovered' } });
        expect(api.startPlayback).toHaveBeenCalledOnce();
        expect(api.startPlayback.mock.calls[0][0]).toMatchObject({ audio_stream_index: 2 });
    });

    it.each(['unknown', 'unsupported'])('does not allocate without positive capability evidence (%s)', async (support) => {
        probe.mockResolvedValueOnce(support);
        await start();
        expect(await player.reportSourceFailure(failure())).toBe(false);
        expect(api.heartbeat).not.toHaveBeenCalled();
        expect(get(player)).toMatchObject({ streamDecision: 'direct_play', isPlaying: false, isBuffering: false, recoveryRequested: false,
            recovery: { status: 'exhausted', reason: 'unavailable' } });
    });

    it.each([2, 4, null])('keeps native code %s out of direct recovery allocation', async (mediaErrorCode) => {
        await start();
        expect(await player.reportSourceFailure({ ...failure(), mediaErrorCode })).toBe(false);
        expect(probe).not.toHaveBeenCalled();
        expect(api.heartbeat).not.toHaveBeenCalled();
    });

    it('rejects stale source events and incomplete descriptors', async () => {
        await start();
        expect(await player.reportSourceFailure({ ...failure(), sourceUrl: '/api/v1/stream/old-file' })).toBe(false);
        expect(get(player).isPlaying).toBe(true);
        player.reset();
        api.startPlayback.mockResolvedValueOnce({ ...direct(), direct_fallback: null });
        await start();
        expect(await player.reportSourceFailure(failure())).toBe(false);
        expect(probe).not.toHaveBeenCalled();
    });

    it('coalesces repeated decode hints behind a normal heartbeat into one bounded follow-up', async () => {
        await start();
        const normal = deferred<any>();
        api.heartbeat.mockReturnValueOnce(normal.promise);
        const heartbeat = player.sendHeartbeatNow();
        await flush();
        const first = player.reportSourceFailure(failure());
        const second = player.reportSourceFailure(failure());
        await flush();
        expect(probe).toHaveBeenCalledOnce();
        expect(api.heartbeat).toHaveBeenCalledOnce();
        expect(api.heartbeat.mock.calls[0][0]).toMatchObject({ direct_source_revision: revision });
        expect(api.heartbeat.mock.calls[0][0]).not.toHaveProperty('recovery_requested');
        normal.resolve({ ...direct(), position_ms: 4500 });
        await Promise.all([heartbeat, first, second]);
        expect(api.heartbeat).toHaveBeenCalledTimes(2);
        expect(api.heartbeat.mock.calls[1][0]).toMatchObject({ direct_fallback_supported: true, recovery_requested: true });
        expect(get(player).sessionId).toBe('session-1');
    });

    it.each(['seek', 'context', 'stop', 'reset'])('cancels a pending capability result after %s', async (action) => {
        await start();
        const pending = deferred<string>();
        probe.mockReturnValueOnce(pending.promise);
        const reporting = player.reportSourceFailure(failure());
        await flush();
        const signal = probe.mock.calls[0][1].signal;
        if (action === 'seek') await player.seek(9000);
        if (action === 'context') player.setContext({ serverOrigin: 'http://other.test', userId: 'user-b' });
        if (action === 'stop') await player.stop();
        if (action === 'reset') player.reset();
        expect(signal.aborted).toBe(true);
        pending.resolve('supported');
        expect(await reporting).toBe(false);
        expect(api.heartbeat).not.toHaveBeenCalled();
        expect(get(player).transcodeSessionId).toBeNull();
    });

    it('preserves an explicit pause made while capability probing is pending', async () => {
        await start();
        const pending = deferred<string>();
        probe.mockReturnValueOnce(pending.promise);
        const reporting = player.reportSourceFailure(failure());
        await flush();
        expect(player.getPlayIntent()).toBe(true);
        player.setPlaying(false);
        expect(player.getPlayIntent()).toBe(false);
        pending.resolve('supported');
        await reporting;
        expect(get(player)).toMatchObject({ transcodeSessionId: 'worker-1', isPlaying: false });
        expect(api.heartbeat.mock.calls[0][0]).toMatchObject({ is_paused: true });
    });

    it('ignores an already-dispatched replacement after seeking and later adopts the canonical generation', async () => {
        await start();
        const pending = deferred<any>();
        api.heartbeat.mockReturnValueOnce(pending.promise);
        const reporting = player.reportSourceFailure(failure());
        await flush();
        api.seek.mockResolvedValueOnce({ transcode_session_id: 'worker-after-seek', stream_decision: 'transcode', stream_offset_ms: 9000, position_ms: 9000 });
        const seeking = player.seek(9000);
        pending.resolve(recovered());
        await Promise.all([reporting, seeking]);
        expect(get(player)).toMatchObject({ transcodeSessionId: 'worker-after-seek', streamOffsetMs: 9000, positionMs: 9000, directSourceRevision: null });
    });

    it('rotates direct seek ownership without changing its URL and replays a lost seek response from heartbeat', async () => {
        await start();
        await player.seek(9000);
        expect(api.seek).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 9000 });
        expect(get(player)).toMatchObject({ streamDecision: 'direct_play', streamUrl: '/api/v1/stream/file-1', directSourceRevision: nextRevision, positionMs: 9000, streamOffsetMs: 0 });
        api.heartbeat.mockResolvedValueOnce({ ...direct(), direct_source_revision: revision, position_ms: 13000 });
        await player.sendHeartbeatNow();
        expect(api.heartbeat.mock.calls[0][0]).toMatchObject({ direct_source_revision: nextRevision });
        expect(get(player)).toMatchObject({ directSourceRevision: revision, positionMs: 13000 });
    });

    it('restores the canonical direct source and its admission descriptor through resume', async () => {
        api.getPlaybackInfo.mockResolvedValueOnce({ ...direct(), media_file_id: 'resumed-file', position_ms: 7000, is_paused: true });
        await player.resume('session-1');
        expect(get(player)).toMatchObject({ mediaFileId: 'resumed-file', streamUrl: '/api/v1/stream/resumed-file', directSourceRevision: revision, directFallback: descriptor, positionMs: 7000, isPlaying: false });
    });

    it.each([false, true])('adopts a canonical direct seek revision with stored exhaustion and explicit pause=%s', async (pause) => {
        await start();
        const exhausted = { status: 'exhausted', attempts: 1, reason: 'replacement_failed', skipped_ms: 0, quality_reduced: false };
        api.heartbeat.mockResolvedValueOnce({ ...direct(), position_ms: 4500, recovery: exhausted });
        await player.reportSourceFailure(failure());
        if (pause) player.setPlaying(false);
        api.seek.mockRejectedValueOnce(new Error('Seek acknowledgement lost'));
        await expect(player.seek(9000)).rejects.toThrow('Seek acknowledgement lost');
        expect(get(player).directSourceRevision).toBe(revision);
        api.heartbeat.mockResolvedValueOnce({ ...direct(), direct_source_revision: nextRevision, position_ms: 9000, recovery: exhausted });
        await player.sendHeartbeatNow();
        expect(get(player)).toMatchObject({ sessionId: 'session-1', directSourceRevision: nextRevision, positionMs: 9000,
            streamUrl: '/api/v1/stream/file-1', streamDecision: 'direct_play', transcodeSessionId: null,
            recovery: exhausted, recoveryRequested: false, directFallbackSupported: false, isPlaying: false, isBuffering: false });
        expect(api.startPlayback).toHaveBeenCalledOnce();
        expect(probe).toHaveBeenCalledOnce();
        expect(api.heartbeat.mock.calls[1][0]).toMatchObject({ direct_source_revision: revision });
        expect(api.heartbeat.mock.calls[1][0]).not.toHaveProperty('recovery_requested');
    });
});
