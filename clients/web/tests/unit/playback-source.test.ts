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
