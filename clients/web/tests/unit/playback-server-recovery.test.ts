import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const api = vi.hoisted(() => ({
    startPlayback: vi.fn<(...args: any[]) => Promise<any>>(), heartbeat: vi.fn<(...args: any[]) => Promise<any>>(),
    stopPlayback: vi.fn<(...args: any[]) => Promise<any>>(), seek: vi.fn<(...args: any[]) => Promise<any>>(),
    getPlaybackInfo: vi.fn<(...args: any[]) => Promise<any>>(),
    streamFileUrl: (id: string) => `/api/v1/stream/${id}`,
    transcodeManifestUrl: (id: string) => `/api/v1/transcode/${id}/manifest.m3u8`,
}));
vi.mock('../../src/lib/api/playback.js', () => api);
import { createPlayerStore } from '../../src/lib/stores/player.js';
import { playbackRecoveryMessage } from '../../src/lib/playback/recovery.js';

const first = '00000000-0000-7000-8000-000000000001';
const second = '00000000-0000-7000-8000-000000000002';
const third = '00000000-0000-7000-8000-000000000003';
const item = { id: 'movie-1', type: 'movie', runtime_seconds: 900 };
let player: ReturnType<typeof createPlayerStore>;

function deferred() {
    let resolve!: (value: any) => void;
    const promise = new Promise<any>((accept) => { resolve = accept; });
    return { promise, resolve };
}

function recovered(extra = {}) {
    return { session_id: 'play-1', position_ms: 127450, transcode_session_id: second, stream_offset_ms: 127450,
        recovery: { status: 'recovered', attempts: 1, reason: 'decode_skip', skipped_ms: 4000, quality_reduced: false }, ...extra };
}

beforeEach(() => {
    vi.useFakeTimers();
    vi.resetAllMocks();
    api.startPlayback.mockResolvedValue({ session_id: 'play-1', stream_decision: 'direct_stream', transcode_session_id: first });
    api.heartbeat.mockResolvedValue({ session_id: 'play-1', position_ms: 0 });
    api.stopPlayback.mockResolvedValue({});
    api.seek.mockResolvedValue({ stream_url: '/replacement.m3u8' });
    player = createPlayerStore();
    player.setContext({ serverOrigin: 'https://media.test', userId: 'account-a' });
});

afterEach(async () => {
    await player.stop();
    player.destroy();
    vi.clearAllTimers();
    vi.useRealTimers();
});

describe('server-authoritative playback recovery', () => {
    it('replaces only the transcode generation at the actual skipped source position', async () => {
        await player.play(item, 'file-1', { startPositionMs: 120000 });
        player.setVolume(0.4);
        player.setPlaybackRate(1.5);
        api.heartbeat.mockResolvedValueOnce(recovered());
        await player.reportSourceFailure({ positionMs: 123450, recoveryRequested: true });
        expect(api.heartbeat).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({
            session_id: 'play-1', transcode_session_id: first, position_ms: 123450, recovery_requested: true,
        }));
        expect(get(player)).toMatchObject({ sessionId: 'play-1', transcodeSessionId: second, streamOffsetMs: 127450,
            positionMs: 127450, isPlaying: true, isBuffering: false, volume: 0.4, playbackRate: 1.5,
            recovery: { status: 'recovered', skipped_ms: 4000, quality_reduced: false } });
        expect(get(player).streamUrl).toBe(api.transcodeManifestUrl(second));
        expect(playbackRecoveryMessage(get(player).recovery)).toBe('tonight_player_recovery_skipped');
        expect(api.startPlayback).toHaveBeenCalledOnce();
        expect(api.stopPlayback).not.toHaveBeenCalled();
    });

    it('keeps paused playback paused when a periodic heartbeat replaces the worker', async () => {
        await player.play(item, 'file-1');
        player.setPlaying(false);
        api.heartbeat.mockResolvedValueOnce(recovered());
        await player.sendHeartbeatNow();
        expect(get(player)).toMatchObject({ transcodeSessionId: second, isPlaying: false });
        expect(api.heartbeat.mock.calls[0][0]).not.toHaveProperty('recovery_requested');
    });

    it('adopts a canonical generation replay without rewinding on ordinary heartbeat positions', async () => {
        await player.play(item, 'file-1');
        api.heartbeat.mockResolvedValueOnce(recovered({ recovery: undefined }));
        await player.sendHeartbeatNow();
        expect(get(player)).toMatchObject({ transcodeSessionId: second, positionMs: 127450, streamOffsetMs: 127450 });
        player.setPosition(150000);
        await player.sendHeartbeatNow();
        expect(get(player).positionMs).toBe(150000);
    });

    it('settles a pending hint when canonical replay omits optional recovery details', async () => {
        await player.play(item, 'file-1');
        api.heartbeat.mockResolvedValueOnce(recovered({ recovery: undefined }));
        await player.reportSourceFailure({ positionMs: 123450, recoveryRequested: true });
        expect(get(player)).toMatchObject({ transcodeSessionId: second, isPlaying: true, recoveryRequested: false, recovery: null });
    });

    it('queues one recovery hint behind an already pending normal heartbeat', async () => {
        await player.play(item, 'file-1');
        const pending = deferred();
        api.heartbeat.mockReturnValueOnce(pending.promise).mockResolvedValueOnce(recovered());
        const normal = player.sendHeartbeatNow();
        const hint = player.reportSourceFailure({ positionMs: 123450, recoveryRequested: true });
        expect(get(player).recovery?.status).toBe('pending');
        pending.resolve({ session_id: 'play-1', position_ms: 0 });
        await Promise.all([normal, hint]);
        expect(api.heartbeat).toHaveBeenCalledTimes(2);
        expect(api.heartbeat.mock.calls[0][0]).not.toHaveProperty('recovery_requested');
        expect(api.heartbeat.mock.calls[1][0]).toMatchObject({ transcode_session_id: first, recovery_requested: true });
        expect(get(player).transcodeSessionId).toBe(second);
    });

    it('drains a preceding heartbeat and ignores its response before a seek takes ownership', async () => {
        await player.play(item, 'file-1');
        const pending = deferred();
        api.heartbeat.mockReturnValueOnce(pending.promise);
        api.seek.mockResolvedValueOnce({ transcode_session_id: third, position_ms: 300000, stream_offset_ms: 300000 });
        const heartbeat = player.sendHeartbeatNow();
        const seek = player.seek(300000);
        expect(api.seek).not.toHaveBeenCalled();
        pending.resolve(recovered());
        await Promise.all([heartbeat, seek]);
        expect(get(player)).toMatchObject({ transcodeSessionId: third, positionMs: 300000, streamOffsetMs: 300000, seeking: false });
    });

    it.each(['stop', 'account', 'server'])('rejects an old recovery response after %s takes ownership', async (change) => {
        await player.play(item, 'file-1');
        const pending = deferred();
        api.heartbeat.mockReturnValueOnce(pending.promise);
        const heartbeat = player.sendHeartbeatNow();
        let stopping: Promise<any> | null = null;
        if (change === 'stop') stopping = player.stop();
        else player.setContext({ serverOrigin: change === 'server' ? 'https://other.test' : 'https://media.test', userId: change === 'account' ? 'account-b' : 'account-a' });
        pending.resolve(recovered());
        await heartbeat;
        await stopping;
        expect(get(player).sessionId).toBeNull();
        expect(get(player).streamUrl).toBeNull();
    });

    it.each([false, true])('settles an ineligible hint and preserves later watchdog intent with explicit pause=%s', async (pause) => {
        await player.play(item, 'file-1');
        api.heartbeat.mockResolvedValueOnce({ session_id: 'play-1', position_ms: 123450,
            recovery: { status: 'exhausted', attempts: 0, reason: 'not_eligible', skipped_ms: 0, quality_reduced: false } });
        await player.reportSourceFailure({ positionMs: 123450, recoveryRequested: true });
        expect(get(player)).toMatchObject({ isPlaying: false, isBuffering: false, recoveryRequested: false, recovery: { status: 'exhausted', attempts: 0 } });
        if (pause) player.setPlaying(false);
        api.heartbeat.mockResolvedValueOnce(recovered());
        await player.sendHeartbeatNow();
        expect(get(player)).toMatchObject({ transcodeSessionId: second, isPlaying: !pause, recovery: { status: 'recovered' } });
    });

    it('settles a failed hint transport without an automatic retry loop', async () => {
        await player.play(item, 'file-1');
        api.heartbeat.mockRejectedValueOnce(new Error('Offline'));
        await player.reportSourceFailure({ positionMs: 123450, recoveryRequested: true });
        expect(get(player)).toMatchObject({ recovery: { status: 'exhausted', reason: 'unavailable' }, recoveryRequested: false, isBuffering: false });
        expect(api.heartbeat).toHaveBeenCalledOnce();
    });

    it('retains startup recovery information without inventing reduced quality', async () => {
        api.startPlayback.mockResolvedValueOnce({ session_id: 'play-1', stream_decision: 'transcode', transcode_session_id: first,
            recovery: { status: 'recovered', attempts: 1, reason: 'hardware_unavailable', skipped_ms: 0, quality_reduced: false } });
        await player.play(item, 'file-1');
        expect(get(player).recovery?.quality_reduced).toBe(false);
        expect(playbackRecoveryMessage(get(player).recovery)).toBe('tonight_player_recovery_compatible');
    });

    it('resumes the canonical worker and offset from playback info', async () => {
        api.getPlaybackInfo.mockResolvedValueOnce({ session_id: 'play-1', stream_decision: 'transcode', transcode_session_id: second,
            position_ms: 130000, stream_offset_ms: 127450, is_paused: true, recovery: recovered().recovery });
        await player.resume('play-1');
        expect(get(player)).toMatchObject({ streamUrl: api.transcodeManifestUrl(second), streamOffsetMs: 127450, positionMs: 130000, isPlaying: false, recovery: { skipped_ms: 4000 } });
    });

    it('rejects a replacement without an authoritative valid timeline offset', async () => {
        await player.play(item, 'file-1');
        player.setPosition(123450);
        api.heartbeat.mockResolvedValueOnce(recovered({ stream_offset_ms: -1 }));
        await player.sendHeartbeatNow();
        expect(get(player)).toMatchObject({ transcodeSessionId: first, positionMs: 123450, streamOffsetMs: 0 });
    });
});
