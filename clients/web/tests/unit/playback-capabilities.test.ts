import { afterEach, describe, expect, it, vi } from 'vitest';
import { directFallbackDescriptor, directSourceRevision, probeCompatibleHlsOutput } from '../../src/lib/playback/capabilities.js';

const output = () => ({
    video: { content_type: 'video/mp4; codecs="avc1.640028"', width: 320, height: 180, bitrate: 500000, framerate: 30 },
    audio: { content_type: 'audio/mp4; codecs="mp4a.40.2"', channels: '2', bitrate: 128000, samplerate: 48000 },
});
type HlsSupport = { isSupported: () => boolean };
const options = () => ({
    loadHls: vi.fn(async (): Promise<HlsSupport> => ({ isSupported: () => true })),
    mediaSource: { isTypeSupported: vi.fn(() => true) },
    mediaCapabilities: { decodingInfo: vi.fn(async (_config: unknown): Promise<any> => ({ supported: true })) },
});

afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe('bounded direct fallback output capability', () => {
    it('requires the exact admitted codec, dimensions, bitrate, frame rate and audio format', () => {
        const descriptor = directFallbackDescriptor(output());
        expect(descriptor).toEqual(output());
        expect(Object.isFrozen(descriptor.video)).toBe(true);
        for (const video of [{ width: 321 }, { height: 1082 }, { bitrate: 10000001 }, { framerate: 29.97 }, { content_type: 'video/mp4; codecs="hvc1"' }]) {
            expect(directFallbackDescriptor({ ...output(), video: { ...output().video, ...video } })).toBeNull();
        }
        for (const audio of [{ channels: '6' }, { samplerate: 44100 }, { bitrate: 192001 }]) {
            expect(directFallbackDescriptor({ ...output(), audio: { ...output().audio, ...audio } })).toBeNull();
        }
        expect(directSourceRevision('00000000-0000-7000-8000-000000000001')).toBeTruthy();
        expect(directSourceRevision('file:///private/source')).toBeNull();
    });

    it('checks exact MSE types and makes a fresh media-source query for every request', async () => {
        const dependencies = options();
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('supported');
        expect(dependencies.mediaSource.isTypeSupported.mock.calls).toEqual([[output().video.content_type], [output().audio.content_type]]);
        expect(dependencies.mediaCapabilities.decodingInfo).toHaveBeenCalledExactlyOnceWith({
            type: 'media-source',
            video: { contentType: output().video.content_type, width: 320, height: 180, bitrate: 500000, framerate: 30 },
            audio: { contentType: output().audio.content_type, channels: '2', bitrate: 128000, samplerate: 48000 },
        });
        dependencies.mediaCapabilities.decodingInfo.mockResolvedValueOnce({ supported: false });
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('unsupported');
        expect(dependencies.mediaCapabilities.decodingInfo).toHaveBeenCalledTimes(2);
    });

    it('rejects unavailable HLS/MSE before querying decoder support', async () => {
        const dependencies = options();
        dependencies.mediaSource.isTypeSupported.mockReturnValueOnce(false);
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('unsupported');
        expect(dependencies.mediaCapabilities.decodingInfo).not.toHaveBeenCalled();
        dependencies.loadHls.mockResolvedValueOnce({ isSupported: () => false });
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('unsupported');
    });

    it.each([undefined, {}, { supported: null }])('treats incomplete support evidence as unknown: %s', async (result) => {
        const dependencies = options();
        dependencies.mediaCapabilities.decodingInfo.mockResolvedValueOnce(result);
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('unknown');
    });

    it('fails closed on rejected queries and malformed descriptors', async () => {
        const dependencies = options();
        dependencies.mediaCapabilities.decodingInfo.mockRejectedValueOnce(new TypeError('Invalid configuration'));
        expect(await probeCompatibleHlsOutput(output(), dependencies)).toBe('unknown');
        expect(await probeCompatibleHlsOutput(null, dependencies)).toBe('unknown');
        expect(dependencies.loadHls).toHaveBeenCalledOnce();
    });

    it('bounds even a stalled HLS import and clears its timer after cancellation', async () => {
        vi.useFakeTimers();
        const dependencies = { ...options(), loadHls: () => new Promise<HlsSupport>(() => {}) };
        const pending = probeCompatibleHlsOutput(output(), { ...dependencies, timeoutMs: 50 });
        await vi.advanceTimersByTimeAsync(50);
        expect(await pending).toBe('unknown');
        expect(vi.getTimerCount()).toBe(0);
        const controller = new AbortController();
        const cancelled = probeCompatibleHlsOutput(output(), { ...dependencies, signal: controller.signal });
        controller.abort();
        expect(await cancelled).toBe('unknown');
        expect(vi.getTimerCount()).toBe(0);
        expect(dependencies.mediaCapabilities.decodingInfo).not.toHaveBeenCalled();
    });
});
