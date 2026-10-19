import { afterEach, describe, expect, it, vi } from 'vitest';
import { createMediaSource } from '../../src/lib/playback/source.js';
import { createPlaybackAnnotations } from '../../src/lib/playback/annotations.js';
import { clearBearerToken, clearServerOrigin, mediaRequestHeaders, setBearerToken, setServerOrigin } from '../../src/lib/api/core.js';

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((accept) => { resolve = accept; });
    return { promise, resolve };
}

function video() {
    return Object.assign(new EventTarget(), { src: '', currentTime: 0, error: null as any, pause: vi.fn(), load: vi.fn(), play: vi.fn(async () => {}), removeAttribute: vi.fn(), canPlayType: vi.fn(() => 'probably') });
}

function hlsHarness() {
    const instances: Hls[] = [];
    class Hls {
        static Events = { MANIFEST_PARSED: 'manifest', LEVEL_UPDATED: 'level', ERROR: 'error' };
        static ErrorTypes = { NETWORK_ERROR: 'networkError', MEDIA_ERROR: 'mediaError' };
        static isSupported() { return true; }
        handlers = new Map<string, (...args: any[]) => void>();
        startLoad = vi.fn();
        recoverMediaError = vi.fn();
        destroy = vi.fn();
        configuration: Record<string, unknown>;
        constructor(configuration: Record<string, unknown> = {}) { this.configuration = configuration; instances.push(this); }
        on(name: string, callback: (...args: any[]) => void) { this.handlers.set(name, callback); }
        loadSource() {}
        attachMedia() {}
        emitError(data: any) { this.handlers.get('error')?.('error', data); }
    }
    return { Hls, instances };
}

afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe('media source lifecycle and credentials', () => {
    it('sets the canonical local seek target before loading copied HLS and preserves paused intent', async () => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, shouldPlay: () => false, requiresHeaders: () => false });
        await source.attach('/api/v1/transcode/copied/manifest.m3u8', { startPosition: (5000 - 3916) / 1000 });
        expect(instances[0].configuration.startPosition).toBe(1.084);
        instances[0].handlers.get('manifest')?.();
        expect(media.play).not.toHaveBeenCalled();
        source.dispose();
    });

    it.each([Number.NaN, Infinity, -1, 2147483.648])('refuses unsafe HLS attachment position %s before allocating an engine', async (startPosition) => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const onError = vi.fn();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError });
        await source.attach('/api/v1/transcode/copied/manifest.m3u8', { startPosition });
        expect(instances).toHaveLength(0);
        expect(onError).toHaveBeenCalledOnce();
        expect(media.play).not.toHaveBeenCalled();
        source.dispose();
    });
    it('reports an actual media error only while its source remains owned', async () => {
        const media = video();
        const failure = vi.fn((_error: unknown) => {});
        const source = createMediaSource({ video: media, onError: failure, requiresHeaders: () => false });
        await source.attach('/api/v1/stream/one');
        media.error = { code: 4 };
        media.dispatchEvent(new Event('error'));
        expect(failure).toHaveBeenCalledExactlyOnceWith(media.error, { url: '/api/v1/stream/one', mode: 'direct', mediaErrorCode: 4, positionSeconds: 0, recoveryRequested: false });
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

    it('captures a fatal HLS media position before unloading and requests bounded server recovery', async () => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn((_error: unknown, _details: any) => {});
        media.load.mockImplementation(() => { media.currentTime = 0; });
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 12.5;
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR });
        expect(instances[0].recoverMediaError).toHaveBeenCalledOnce();
        expect(failure).not.toHaveBeenCalled();
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR });
        expect(failure.mock.calls[0][1]).toEqual({ url: '/api/v1/transcode/one/manifest.m3u8', mode: 'hls-mse', mediaErrorCode: null, positionSeconds: 12.5, recoveryRequested: true });
        expect(media.currentTime).toBe(0);
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR });
        expect(failure).toHaveBeenCalledOnce();
        source.dispose();
    });

    it('bounds a native HLS decode failure without a library error and retains its position across local reload', async () => {
        vi.useFakeTimers();
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 12.5;
        const error = { code: 3 };
        media.error = error;
        instances[0].recoverMediaError.mockImplementation(() => { media.currentTime = 0; media.error = null; });
        media.dispatchEvent(new Event('error'));
        expect(instances[0].recoverMediaError).toHaveBeenCalledOnce();
        expect(failure).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(4999);
        expect(failure).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(1);
        expect(failure).toHaveBeenCalledExactlyOnceWith(error, { url: '/api/v1/transcode/one/manifest.m3u8', mode: 'hls-mse', mediaErrorCode: 3, positionSeconds: 12.5, recoveryRequested: true });
        expect(instances[0].destroy).toHaveBeenCalledOnce();
        source.dispose();
    });

    it.each(['native', 'hls'])('coalesces paired native/library reports starting with %s and permits only one local retry', async (origin) => {
        vi.useFakeTimers();
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 6.5;
        media.error = { code: 3 };
        const native = () => media.dispatchEvent(new Event('error'));
        const library = () => instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR });
        if (origin === 'native') { native(); library(); } else { library(); native(); }
        expect(instances[0].recoverMediaError).toHaveBeenCalledOnce();
        expect(failure).not.toHaveBeenCalled();
        media.error = null;
        media.currentTime = 0;
        media.dispatchEvent(new Event('loadedmetadata'));
        media.error = { code: 3 };
        native();
        expect(failure.mock.calls[0][1]).toMatchObject({ mode: 'hls-mse', mediaErrorCode: 3, positionSeconds: 6.5, recoveryRequested: true });
        library();
        await vi.advanceTimersByTimeAsync(5000);
        expect(failure).toHaveBeenCalledOnce();
        expect(instances[0].recoverMediaError).toHaveBeenCalledOnce();
        source.dispose();
    });

    it.each([0, 1, 2, 4])('never turns native HLS code %s into a decode retry or server hint', async (code) => {
        vi.useFakeTimers();
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.error = { code };
        media.dispatchEvent(new Event('error'));
        await vi.advanceTimersByTimeAsync(5000);
        expect(instances[0].recoverMediaError).not.toHaveBeenCalled();
        expect(failure).not.toHaveBeenCalled();
        source.dispose();
    });

    it('uses live pause intent on local retry and cancels its deadline when the source is replaced', async () => {
        vi.useFakeTimers();
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn();
        let playing = true;
        const source = createMediaSource({ video: media, loadHls: async () => Hls, shouldPlay: () => playing, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 2.25;
        media.error = { code: 3 };
        media.dispatchEvent(new Event('error'));
        playing = false;
        media.error = null;
        media.dispatchEvent(new Event('loadedmetadata'));
        expect(media.play).not.toHaveBeenCalled();
        await source.attach('/api/v1/transcode/two/manifest.m3u8', { autoplay: false });
        await vi.advanceTimersByTimeAsync(5000);
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR });
        expect(failure).not.toHaveBeenCalled();
        expect(instances).toHaveLength(2);
        source.dispose();
    });

    it('clears the local retry deadline only after decoded progress and retains the exhausted retry budget', async () => {
        vi.useFakeTimers();
        const media = Object.assign(video(), { readyState: 4 });
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 2;
        media.error = { code: 3 };
        media.dispatchEvent(new Event('error'));
        media.error = null;
        media.dispatchEvent(new Event('loadedmetadata'));
        expect(media.play).toHaveBeenCalledOnce();
        media.currentTime = 2.1;
        media.dispatchEvent(new Event('timeupdate'));
        await vi.advanceTimersByTimeAsync(5000);
        expect(failure).not.toHaveBeenCalled();
        media.error = { code: 3 };
        media.dispatchEvent(new Event('error'));
        expect(failure.mock.calls[0][1]).toMatchObject({ positionSeconds: 2.1, recoveryRequested: true });
        expect(instances[0].recoverMediaError).toHaveBeenCalledOnce();
        source.dispose();
    });

    it.each([401, 403])('never retries or requests server recovery for HLS status %s', async (code) => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn((_error: unknown, _details: any) => {});
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.MEDIA_ERROR, response: { code } });
        expect(instances[0].recoverMediaError).not.toHaveBeenCalled();
        expect(instances[0].startLoad).not.toHaveBeenCalled();
        expect(failure.mock.calls[0][1].recoveryRequested).toBe(false);
        source.dispose();
    });

    it('keeps a network-only failure out of server media recovery hints', async () => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const failure = vi.fn((_error: unknown, _details: any) => {});
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.NETWORK_ERROR });
        instances[0].emitError({ fatal: true, type: Hls.ErrorTypes.NETWORK_ERROR });
        expect(instances[0].startLoad).toHaveBeenCalledOnce();
        expect(failure.mock.calls[0][1].recoveryRequested).toBe(false);
        source.dispose();
    });

    it('preserves paused intent when a replacement HLS manifest is parsed', async () => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const source = createMediaSource({ video: media, loadHls: async () => Hls, requiresHeaders: () => false });
        await source.attach('/api/v1/transcode/one/manifest.m3u8', { autoplay: false });
        instances[0].handlers.get('manifest')?.();
        expect(media.play).not.toHaveBeenCalled();
        source.dispose();
    });

    it('reads a pause made while the replacement HLS import is pending', async () => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        const pending = deferred<typeof Hls>();
        let playing = true;
        const source = createMediaSource({ video: media, loadHls: () => pending.promise, shouldPlay: () => playing, requiresHeaders: () => false });
        const attaching = source.attach('/api/v1/transcode/replacement/manifest.m3u8');
        playing = false;
        pending.resolve(Hls);
        await attaching;
        instances[0].handlers.get('manifest')?.();
        expect(media.play).not.toHaveBeenCalled();
        source.dispose();
    });

    it.each([false, true])('uses the latest play intent=%s after attaching and before parsing the replacement manifest', async (playing) => {
        const media = video();
        const { Hls, instances } = hlsHarness();
        let intent = !playing;
        const source = createMediaSource({ video: media, loadHls: async () => Hls, shouldPlay: () => intent, requiresHeaders: () => false });
        await source.attach('/api/v1/transcode/replacement/manifest.m3u8');
        intent = playing;
        instances[0].handlers.get('manifest')?.();
        expect(media.play).toHaveBeenCalledTimes(playing ? 1 : 0);
        source.dispose();
    });

    it('classifies a native decode error with its retained media position', async () => {
        const media = video();
        const failure = vi.fn((_error: unknown, _details: any) => {});
        const source = createMediaSource({ video: media, loadHls: async () => ({ isSupported: () => false }), requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/transcode/one/manifest.m3u8');
        media.currentTime = 4.25;
        media.error = { code: 3 };
        media.dispatchEvent(new Event('error'));
        expect(failure.mock.calls[0][1]).toMatchObject({ positionSeconds: 4.25, recoveryRequested: true });
        source.dispose();
    });

    it('retains the direct delivery identity and typed decode code before unloading', async () => {
        const media = video();
        const failure = vi.fn((_error: unknown, _details: any) => {});
        media.load.mockImplementation(() => { media.currentTime = 0; });
        const source = createMediaSource({ video: media, requiresHeaders: () => false, onError: failure });
        await source.attach('/api/v1/stream/direct');
        media.currentTime = 7.5;
        media.error = { code: 3 };
        media.dispatchEvent(new Event('error'));
        expect(failure.mock.calls[0][1]).toEqual({ url: '/api/v1/stream/direct', mode: 'direct', mediaErrorCode: 3, positionSeconds: 7.5, recoveryRequested: true });
        expect(media.currentTime).toBe(0);
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
