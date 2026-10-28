import { afterEach, describe, expect, it, vi } from 'vitest';
import { boundedDecodingObservations, createDecodingProbe, decodingAdmitted } from '../../src/lib/playback/decoding-capabilities.js';
import { prepareSourceCapabilities, sourcePreparation, sourceRoute } from '../../src/lib/playback/source-capabilities.js';
import { playbackStartRequest } from '../../src/lib/playback/start-request.js';

const revision = 'a'.repeat(64);
const config = (type = 'media-source') => ({ type,
    video: { contentType: 'video/mp4; codecs="av01.0.00M.10"', width: 320, height: 180, bitrate: 500000, framerate: 24, colorGamut: 'srgb' },
    audio: { contentType: 'audio/mp4; codecs="mp4a.40.2"', channels: '2', bitrate: 128000, samplerate: 48000 },
});
const prepared = () => ({ protocol: 1, source_revision: revision,
    candidates: [{ id: 'native_file', delivery: 'file', configuration: config('file') },
        { id: 'hls_video_copy_aac', delivery: 'hls', configuration: config() }],
});
const evidence = () => ({ status: 'reported', supported: true, smooth: true, power_efficient: false, delivery_supported: true });
const dependencies = () => ({
    mediaCapabilities: { decodingInfo: vi.fn(async (_value: unknown): Promise<MediaCapabilitiesDecodingInfo> => ({ supported: true, smooth: true, powerEfficient: false, keySystemAccess: null })) },
    mediaSource: { isTypeSupported: vi.fn((_type: string) => true) },
    createMediaElement: () => ({ canPlayType: (_type: string): CanPlayTypeResult => 'probably' }),
    loadHls: vi.fn(async () => ({ isSupported: () => true })),
});

afterEach(() => { vi.clearAllTimers(); vi.useRealTimers(); });

describe('exact current-attempt source capability admission', () => {
    it('retains the exact supplied file/MSE configurations and independent power evidence', async () => {
        const deps = dependencies();
        const configurations = [config('file'), config()];
        const values = await boundedDecodingObservations(configurations, { probe: createDecodingProbe(deps) });
        expect(values).toEqual([evidence(), evidence()]);
        expect(deps.mediaCapabilities.decodingInfo.mock.calls.map(([value]) => value)).toEqual(configurations);
        expect(deps.mediaSource.isTypeSupported.mock.calls).toEqual([[config().video.contentType], [config().audio.contentType]]);
        expect(values.every(decodingAdmitted)).toBe(true);
    });

    it('does not infer MSE delivery from positive file decoding support', async () => {
        const deps = dependencies();
        deps.mediaSource.isTypeSupported.mockReturnValue(false);
        const values = await boundedDecodingObservations([config('file'), config()], { probe: createDecodingProbe(deps) });
        expect(values[0]).toEqual(evidence());
        expect(values[1]).toEqual({ ...evidence(), delivery_supported: false });
        expect(decodingAdmitted(values[1])).toBe(false);
    });

    it.each([{ supported: false }, { smooth: false }, { smooth: null }, { supported: null }, { delivery_supported: null }])('refuses incomplete or negative evidence %j', (value) => {
        expect(decodingAdmitted({ ...evidence(), ...value })).toBe(false);
    });

    it('bounds all three queries with one deadline and ignores their late results', async () => {
        vi.useFakeTimers();
        let resolve!: (value: ReturnType<typeof evidence>) => void;
        const slow = new Promise<ReturnType<typeof evidence>>((accept) => { resolve = accept; });
        const probe = vi.fn(async (_value: unknown) => slow);
        const pending = boundedDecodingObservations([config(), config(), config()], { probe });
        await vi.advanceTimersByTimeAsync(1500);
        const values = await pending;
        expect(values.map((value) => value.status)).toEqual(['timed_out', 'timed_out', 'timed_out']);
        expect(probe).toHaveBeenCalledTimes(3);
        expect(vi.getTimerCount()).toBe(0);
        resolve(evidence());
        await Promise.resolve();
        expect(values.every((value) => value.status === 'timed_out')).toBe(true);
    });

    it('returns promptly on cancellation and distinguishes missing APIs from rejected queries', async () => {
        const deps = dependencies();
        const controller = new AbortController();
        const slow = boundedDecodingObservations([config()], { signal: controller.signal, probe: async () => new Promise<ReturnType<typeof evidence>>(() => {}) });
        controller.abort();
        expect((await slow)[0].status).toBe('unavailable');
        deps.mediaCapabilities.decodingInfo.mockResolvedValueOnce(undefined);
        expect((await boundedDecodingObservations([config()], { probe: createDecodingProbe(deps) }))[0].status).toBe('unavailable');
        deps.mediaCapabilities.decodingInfo.mockRejectedValueOnce(new TypeError('Invalid exact configuration'));
        expect((await boundedDecodingObservations([config()], { probe: createDecodingProbe(deps) }))[0].status).toBe('rejected');
    });

    it('validates finite descriptors and forbids native file delivery for bearer or mapped tracks', () => {
        expect(sourcePreparation(prepared())).toEqual(prepared());
        expect(sourcePreparation(prepared(), { requiresHeaders: true })).toBeNull();
        expect(sourcePreparation(prepared(), { forceHls: true })).toBeNull();
        for (const mutate of [
            (value: ReturnType<typeof prepared>) => { value.candidates.push(value.candidates[0]); },
            (value: ReturnType<typeof prepared>) => { value.source_revision = 'private/path'; },
            (value: ReturnType<typeof prepared>) => { value.candidates[0].configuration.video.width = 0; },
            (value: ReturnType<typeof prepared>) => { Object.assign(value.candidates[0].configuration.video, { transferFunction: 'pq' }); },
            (value: ReturnType<typeof prepared>) => { Object.assign(value.candidates[0].configuration, { keySystemConfiguration: {} }); },
        ]) { const value = prepared(); mutate(value); expect(sourcePreparation(value)).toBeNull(); }
        expect(sourceRoute({ candidate_id: 'hls_video_copy_aac', source_revision: revision })).toEqual({ candidate_id: 'hls_video_copy_aac', source_revision: revision });
    });

    it('prepares the normalized Start selection and sends observations only for that source revision', async () => {
        const options = { audioStreamIndex: 2, forceHls: true, qualityMode: 'auto', maxBitrate: 6000000 };
        const reply = prepared();
        reply.candidates.shift();
        const prepare = vi.fn(async (_body: unknown, _options: { signal?: AbortSignal }) => reply);
        const observe = vi.fn(async (_values: unknown, _options: { signal?: AbortSignal }) => [evidence()]);
        const value = await prepareSourceCapabilities('item', 'file', options, { prepare, observe, requiresHeaders: true });
        expect(prepare).toHaveBeenCalledExactlyOnceWith({ ...playbackStartRequest('item', 'file', options), capability_protocol: 1 }, { signal: expect.any(AbortSignal), cache: 'no-store', maxJsonBytes: 32768 });
        expect(observe).toHaveBeenCalledExactlyOnceWith([reply.candidates[0].configuration], { signal: expect.any(AbortSignal) });
        expect(prepare.mock.calls[0][1].signal).toBe(observe.mock.calls[0][1].signal);
        expect(value).toEqual({ version: 1, source_revision: revision, observations: [{ candidate_id: 'hls_video_copy_aac', ...evidence() }] });
        await prepareSourceCapabilities('item', 'file', options, { prepare, observe, requiresHeaders: true });
        expect(prepare).toHaveBeenCalledTimes(2);
        expect(observe).toHaveBeenCalledTimes(2);
    });

    it('keeps missing/legacy preparation conservative but propagates authorization and cancellation', async () => {
        const prepare = vi.fn(async (_body: unknown, _options: unknown): Promise<unknown> => null);
        const observe = vi.fn(async (_values: unknown, _options: unknown) => [evidence()]);
        expect(await prepareSourceCapabilities('item', 'file', {}, { prepare, observe })).toBeNull();
        expect(observe).not.toHaveBeenCalled();
        prepare.mockRejectedValueOnce({ status: 404 });
        expect(await prepareSourceCapabilities('item', 'file', {}, { prepare, observe })).toBeNull();
        prepare.mockRejectedValueOnce({ status: 403 });
        await expect(prepareSourceCapabilities('item', 'file', {}, { prepare, observe })).rejects.toMatchObject({ status: 403 });
        const controller = new AbortController();
        controller.abort();
        await expect(prepareSourceCapabilities('item', 'file', {}, { prepare, observe, signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' });
        expect(prepare).toHaveBeenCalledTimes(3);
    });

    it('bounds a missing prepare response and ignores a late response without querying capabilities', async () => {
        vi.useFakeTimers();
        let resolve!: (value: ReturnType<typeof prepared>) => void;
        const response = new Promise<ReturnType<typeof prepared>>((accept) => { resolve = accept; });
        const prepare = vi.fn(async (_body: unknown, _options: { signal?: AbortSignal }) => response);
        const observe = vi.fn(async (_values: unknown, _options: unknown) => [evidence()]);
        const pending = prepareSourceCapabilities('item', 'file', {}, { prepare, observe });
        await vi.advanceTimersByTimeAsync(4000);
        expect(await pending).toBeNull();
        expect(prepare.mock.calls[0][1].signal.aborted).toBe(true);
        resolve(prepared());
        await Promise.resolve();
        expect(observe).not.toHaveBeenCalled();
        expect(vi.getTimerCount()).toBe(0);
    });

    it('cancels a stalled prepare immediately even when the transport ignores abort', async () => {
        vi.useFakeTimers();
        const controller = new AbortController();
        const prepare = vi.fn(async (_body: unknown, _options: unknown) => new Promise<ReturnType<typeof prepared>>(() => {}));
        const pending = prepareSourceCapabilities('item', 'file', {}, { prepare, signal: controller.signal });
        const rejected = expect(pending).rejects.toMatchObject({ name: 'AbortError' });
        controller.abort();
        await rejected;
        expect(vi.getTimerCount()).toBe(0);
    });
});
