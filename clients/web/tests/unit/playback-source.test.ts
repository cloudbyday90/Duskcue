import { describe, expect, it, vi } from 'vitest';
import { createMediaSource } from '../../src/lib/playback/source.js';
import { createPlaybackAnnotations } from '../../src/lib/playback/annotations.js';
import { clearBearerToken, clearServerOrigin, mediaRequestHeaders, setBearerToken, setServerOrigin } from '../../src/lib/api/core.js';

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((accept) => { resolve = accept; });
    return { promise, resolve };
}

function video() {
    return Object.assign(new EventTarget(), { src: '', error: null as any, pause: vi.fn(), load: vi.fn(), play: vi.fn(async () => {}), removeAttribute: vi.fn(), canPlayType: vi.fn(() => 'probably') });
}

describe('media source lifecycle and credentials', () => {
    it('reports an actual media error only while its source remains owned', async () => {
        const media = video();
        const failure = vi.fn((_error: unknown) => {});
        const source = createMediaSource({ video: media, onError: failure, requiresHeaders: () => false });
        await source.attach('/api/v1/stream/one');
        media.error = { code: 4 };
        media.dispatchEvent(new Event('error'));
        expect(failure).toHaveBeenCalledExactlyOnceWith(media.error);
        source.release();
        media.dispatchEvent(new Event('error'));
        source.dispose();
        media.dispatchEvent(new Event('error'));
        expect(failure).toHaveBeenCalledOnce();
    });

    it('reports blocked autoplay as paused without claiming a stream failure', async () => {
        const media = video();
        media.play.mockRejectedValueOnce(new DOMException('Activation required', 'NotAllowedError'));
        const blocked = vi.fn((_error: unknown) => {});
        const failure = vi.fn((_error: unknown) => {});
        const source = createMediaSource({ video: media, onError: failure, onPlaybackBlocked: blocked, requiresHeaders: () => false });
        await source.attach('/api/v1/stream/one');
        await Promise.resolve();
        expect(blocked).toHaveBeenCalledOnce();
        expect(failure).not.toHaveBeenCalled();
        source.dispose();
    });
    it('cannot attach an HLS import that finishes after disposal', async () => {
        const pending = deferred<any>();
        const media = video();
        const Hls = vi.fn();
        const source = createMediaSource({ video: media, loadHls: () => pending.promise });
        const attaching = source.attach('/api/v1/transcode/one/manifest.m3u8');
        source.dispose();
        pending.resolve(Hls);
        await attaching;
        expect(Hls).not.toHaveBeenCalled();
        expect(media.play).not.toHaveBeenCalled();
        expect(media.removeAttribute).toHaveBeenCalledWith('src');
    });

    it('rejects a bearer source without MSE rather than using unauthenticated native HLS', async () => {
        const media = video();
        const failure = vi.fn((_error: unknown) => {});
        const source = createMediaSource({ video: media, loadHls: async () => ({ isSupported: () => false }), requiresHeaders: () => true, onError: failure });
        await source.attach('https://selected.example/api/v1/transcode/one/manifest.m3u8');
        expect(failure).toHaveBeenCalledOnce();
        expect(media.src).toBe('');
        expect(media.play).not.toHaveBeenCalled();
        source.dispose();
    });

    it('uses native HLS when cookie authentication and native support are available', async () => {
        const media = video();
        const source = createMediaSource({ video: media, loadHls: async () => ({ isSupported: () => false }), requiresHeaders: () => false });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        expect(media.src).toBe('/api/v1/transcode/one/manifest.m3u8');
        expect(media.play).toHaveBeenCalledOnce();
        source.dispose();
    });

    it('observes progressive and final playlists without turning publication into playback completion', async () => {
        const media = video();
        const changes = vi.fn((_timeline: unknown) => {});
        const handlers = new Map<string, (...args: any[]) => void>();
        const destroyed = vi.fn();
        class Hls {
            static Events = { MANIFEST_PARSED: 'manifest', LEVEL_UPDATED: 'level', ERROR: 'error' };
            static isSupported() { return true; }
            on(name: string, handler: (...args: any[]) => void) { handlers.set(name, handler); }
            loadSource() {}
            attachMedia() {}
            destroy() { destroyed(); handlers.clear(); }
        }
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onTimelineChange: changes });
        await source.attach('/api/v1/transcode/progressive/manifest.m3u8');
        handlers.get('manifest')!();
        expect(media.play).toHaveBeenCalledOnce();
        handlers.get('level')!('level', { details: { type: 'EVENT', live: true, totalduration: 6, edge: 6 } });
        expect(source.getTimeline()).toMatchObject({ mode: 'hls-mse', playlistType: 'EVENT', complete: false, producedDurationMs: 6000 });
        handlers.get('level')!('level', { details: { type: 'EVENT', live: false, totalduration: 20, edge: 20 } });
        expect(source.getTimeline()).toMatchObject({ complete: true, streamEndMs: 20_000 });
        expect(media.play).toHaveBeenCalledOnce();
        source.dispose();
        expect(destroyed).toHaveBeenCalledOnce();
        expect(source.getTimeline().mode).toBe('detached');
    });

    it('never returns bearer headers for a foreign host or non-API URL', () => {
        setServerOrigin('https://selected.example');
        setBearerToken('test-session');
        try {
            expect(mediaRequestHeaders('https://selected.example/api/v1/transcode/one/seg.m4s')).toEqual({ Authorization: 'Bearer test-session' });
            expect(() => mediaRequestHeaders('https://foreign.example/api/v1/transcode/one/seg.m4s')).toThrow();
            expect(() => mediaRequestHeaders('https://selected.example/foreign')).toThrow();
        } finally { clearBearerToken(); clearServerOrigin(); }
    });
});

describe('source-scoped annotations', () => {
    it('ignores late reads for a preceding episode and does not erase current annotations', async () => {
        const pending = deferred<{ segments: { id: string }[] }>();
        const change = vi.fn((_state: unknown) => {});
        const service = createPlaybackAnnotations({ onChange: change, segments: vi.fn(async (id: string) => id === 'old' ? pending.promise : { segments: [{ id: 'current' }] }), storyboard: vi.fn(async (id: string) => ({ id })) });
        const old = service.load('old', 'old-file');
        await service.load('current', 'current-file');
        pending.resolve({ segments: [{ id: 'old' }] });
        await old;
        expect(change).toHaveBeenLastCalledWith({ segments: [{ id: 'current' }], storyboard: { id: 'current' } });
        service.dispose();
    });
});
