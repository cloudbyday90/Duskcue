import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { copyFallbackDescriptor, probeCopyFallback } from '../../src/lib/playback/copy-fallback.js';

const api = vi.hoisted(() => ({
    startPlayback: vi.fn(async (_body: unknown): Promise<Record<string, unknown>> => ({})),
    heartbeat: vi.fn(async (_body: unknown): Promise<Record<string, unknown>> => ({})),
    stopPlayback: vi.fn(async (_body: unknown) => undefined),
    seek: vi.fn(async (_body: unknown): Promise<Record<string, unknown>> => ({})),
    getPlaybackInfo: vi.fn(async (_id: unknown): Promise<Record<string, unknown>> => ({})),
    streamFileUrl: (id: string) => `/api/v1/stream/${id}`,
    transcodeManifestUrl: (id: string) => `/api/v1/transcode/${id}/manifest.m3u8`,
}));
vi.mock('../../src/lib/api/playback.js', () => api);
import { createPlayerStore } from '../../src/lib/stores/player.js';

const generation = '00000000-0000-7000-8000-000000000001';
const nextGeneration = '00000000-0000-7000-8000-000000000002';
const fallback = (id = generation) => ({ generation: id,
    video: { content_type: 'video/mp4; codecs="avc1.640028"', width: 320, height: 180, bitrate: 75000, framerate: 30 },
    audio: { content_type: 'audio/mp4; codecs="mp4a.40.2"', channels: '2', bitrate: 128000, samplerate: 48000 },
});
const route = { candidate_id: 'hls_video_copy_aac', source_revision: 'a'.repeat(64) };
const source = () => ({ session_id: 'session-1', stream_decision: 'direct_stream', transcode_session_id: generation,
    source_route: route, copy_fallback: fallback(), position_ms: 0, stream_offset_ms: 0 });
const replacement = () => ({ ...source(), stream_decision: 'transcode', transcode_session_id: nextGeneration,
    copy_fallback: null, position_ms: 4500, stream_offset_ms: 4500,
    recovery: { status: 'recovered', attempts: 1, reason: 'client_decode', skipped_ms: 0, quality_reduced: true } });
const failure = () => ({ positionMs: 4500, sourceUrl: `/api/v1/transcode/${generation}/manifest.m3u8`, sourceMode: 'hls-mse', recoveryRequested: true });
const observation = () => ({ status: 'reported', supported: true, smooth: true, power_efficient: false, delivery_supported: true });
function deferred<T>() { let resolve!: (value: T) => void; const promise = new Promise<T>((accept) => { resolve = accept; }); return { promise, resolve }; }
const flush = async () => { for (let index = 0; index < 20; index += 1) await Promise.resolve(); };
const createProbe = () => vi.fn(async (_value: unknown, _options: { signal?: AbortSignal } = {}) => true);
let player: ReturnType<typeof createPlayerStore>;
let probe: ReturnType<typeof createProbe>;
const start = () => player.play({ id: 'item-1', runtime_seconds: 120 }, 'file-1', { audioStreamIndex: 2 });

beforeEach(() => {
    vi.useFakeTimers();
    vi.resetAllMocks();
    api.startPlayback.mockResolvedValue(source());
    api.heartbeat.mockResolvedValue(replacement());
    api.seek.mockResolvedValue({ ...source(), transcode_session_id: nextGeneration, copy_fallback: fallback(nextGeneration), position_ms: 9000 });
    probe = createProbe();
    player = createPlayerStore({ probeCopy: probe });
    player.setContext({ serverOrigin: 'http://fixture.test', userId: 'user-a' });
});
afterEach(() => { player.destroy(); player.reset(); vi.clearAllTimers(); vi.useRealTimers(); });

describe('generation-owned copied HLS decoder downgrade', () => {
    it('requires a fresh exact compatible output query with both support and smoothness', async () => {
        const observe = vi.fn(async (_configs: unknown, _options: unknown) => [observation()]);
        expect(copyFallbackDescriptor(fallback(), generation)).toEqual(fallback());
        expect(copyFallbackDescriptor(fallback(), nextGeneration)).toBeNull();
        expect(await probeCopyFallback(fallback(), { observe })).toBe(true);
        expect(observe).toHaveBeenCalledExactlyOnceWith([{
            type: 'media-source', video: { contentType: fallback().video.content_type, width: 320, height: 180, bitrate: 75000, framerate: 30 },
            audio: { contentType: fallback().audio.content_type, channels: '2', bitrate: 128000, samplerate: 48000 },
        }], { signal: undefined });
        for (const value of [{ smooth: false }, { supported: false }, { delivery_supported: null }, { status: 'timed_out' }]) {
            observe.mockResolvedValueOnce([{ ...observation(), ...value }]);
            expect(await probeCopyFallback(fallback(), { observe })).toBe(false);
        }
    });

    it.each([null, 3])('probes once for HLS error code %s and adopts the encoded replacement atomically under the existing session', async (mediaErrorCode) => {
        await start();
        expect(await player.reportSourceFailure({ ...failure(), mediaErrorCode })).toBe(true);
        expect(probe).toHaveBeenCalledExactlyOnceWith(fallback(), expect.objectContaining({ signal: expect.any(AbortSignal) }));
        expect(api.heartbeat).toHaveBeenCalledExactlyOnceWith({ session_id: 'session-1', position_ms: 4500, state: 'paused', is_buffering: true,
            transcode_session_id: generation, recovery_requested: true, copy_fallback: { generation, supported: true, smooth: true } });
        expect(get(player)).toMatchObject({ sessionId: 'session-1', streamDecision: 'transcode', transcodeSessionId: nextGeneration,
            sourceRoute: route, copyFallback: null, copyFallbackSupported: false, positionMs: 4500, streamOffsetMs: 4500,
            isPlaying: true, recovery: { attempts: 1, skipped_ms: 0, quality_reduced: true } });
        expect(api.startPlayback).toHaveBeenCalledOnce();
    });

    it('does not send a downgrade hint without positive fresh output admission', async () => {
        await start();
        probe.mockResolvedValueOnce(false);
        expect(await player.reportSourceFailure(failure())).toBe(false);
        expect(api.heartbeat).not.toHaveBeenCalled();
        expect(get(player)).toMatchObject({ isPlaying: false, isBuffering: false, recoveryRequested: false, recovery: { status: 'exhausted' } });
    });

    it.each([{ sourceMode: 'direct', mediaErrorCode: 3 }, { sourceMode: 'hls-mse', mediaErrorCode: 0 }, { sourceMode: 'hls-mse', mediaErrorCode: 2 },
        { sourceMode: 'hls-mse', mediaErrorCode: 4 }, { sourceMode: 'native-hls', mediaErrorCode: 4 },
        { sourceMode: 'native-hls', mediaErrorCode: 2 }, { recoveryRequested: false }, { sourceUrl: '/old/manifest.m3u8' }])('keeps stale/auth/network/untyped source errors outside copy downgrade (%j)', async (details) => {
        await start();
        expect(await player.reportSourceFailure({ ...failure(), ...details })).toBe(false);
        expect(probe).not.toHaveBeenCalled();
        expect(api.heartbeat).not.toHaveBeenCalled();
    });

    it.each(['seek', 'context', 'stop', 'reset'])('cancels a pending copy probe after %s and ignores late admission', async (action) => {
        await start();
        const pending = deferred<boolean>();
        probe.mockReturnValueOnce(pending.promise);
        const reporting = player.reportSourceFailure(failure());
        await flush();
        const signal = probe.mock.calls[0][1].signal;
        if (action === 'seek') await player.seek(9000);
        if (action === 'context') player.setContext({ serverOrigin: 'http://other.test', userId: 'user-b' });
        if (action === 'stop') await player.stop();
        if (action === 'reset') player.reset();
        expect(signal.aborted).toBe(true);
        pending.resolve(true);
        expect(await reporting).toBe(false);
        expect(api.heartbeat).not.toHaveBeenCalled();
    });

    it('retains an explicit pause made during probing through canonical replacement adoption', async () => {
        await start();
        const pending = deferred<boolean>();
        probe.mockReturnValueOnce(pending.promise);
        const reporting = player.reportSourceFailure(failure());
        await flush();
        player.setPlaying(false);
        pending.resolve(true);
        await reporting;
        expect(api.heartbeat.mock.calls[0][0]).toMatchObject({ is_paused: true, copy_fallback: { generation, supported: true, smooth: true } });
        expect(get(player).isPlaying).toBe(false);
    });

    it('coalesces hints joining a normal heartbeat and keeps the fresh admission for one follow-up', async () => {
        await start();
        const normal = deferred<Record<string, unknown>>();
        api.heartbeat.mockReturnValueOnce(normal.promise);
        const heartbeat = player.sendHeartbeatNow();
        await flush();
        const first = player.reportSourceFailure(failure());
        const second = player.reportSourceFailure(failure());
        await flush();
        expect(probe).toHaveBeenCalledOnce();
        expect(api.heartbeat).toHaveBeenCalledOnce();
        normal.resolve(source());
        await Promise.all([heartbeat, first, second]);
        expect(api.heartbeat).toHaveBeenCalledTimes(2);
        expect(api.heartbeat.mock.calls[1][0]).toMatchObject({ recovery_requested: true, copy_fallback: { generation, supported: true, smooth: true } });
    });

    it('adopts the current copy descriptor after seek and session resume, never the preceding generation', async () => {
        await start();
        await player.seek(9000);
        expect(get(player)).toMatchObject({ transcodeSessionId: nextGeneration, sourceRoute: route, copyFallback: fallback(nextGeneration) });
        api.getPlaybackInfo.mockResolvedValueOnce({ ...source(), is_paused: true });
        await player.resume('session-1');
        expect(get(player)).toMatchObject({ transcodeSessionId: generation, copyFallback: fallback(), sourceRoute: route, isPlaying: false });
    });
});
