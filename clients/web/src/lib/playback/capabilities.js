/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

const VIDEO_TYPE = 'video/mp4; codecs="avc1.640028"';
const AUDIO_TYPE = 'audio/mp4; codecs="mp4a.40.2"';

export function directSourceRevision(value) {
    return typeof value === 'string' && /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(value) ? value : null;
}

export function directFallbackDescriptor(value) {
    const video = value?.video;
    const audio = value?.audio;
    if (!video || !audio || video.content_type !== VIDEO_TYPE || audio.content_type !== AUDIO_TYPE
        || !Number.isSafeInteger(video.width) || video.width < 2 || video.width > 1920 || video.width % 2
        || !Number.isSafeInteger(video.height) || video.height < 2 || video.height > 1080 || video.height % 2
        || !Number.isSafeInteger(video.bitrate) || video.bitrate < 1000 || video.bitrate > 10000000 || video.framerate !== 30
        || !['1', '2'].includes(audio.channels) || audio.samplerate !== 48000
        || !Number.isSafeInteger(audio.bitrate) || audio.bitrate < 32000 || audio.bitrate > 192000) return null;
    return Object.freeze({
        video: Object.freeze({ content_type: VIDEO_TYPE, width: video.width, height: video.height, bitrate: video.bitrate, framerate: 30 }),
        audio: Object.freeze({ content_type: AUDIO_TYPE, channels: audio.channels, bitrate: audio.bitrate, samplerate: 48000 }),
    });
}

export async function probeCompatibleHlsOutput(descriptor, {
    loadHls = async () => {
        const { default: Hls } = await import('hls.js');
        return { isSupported: () => Hls.isSupported() };
    },
    mediaCapabilities = { decodingInfo: (configuration) => globalThis.navigator?.mediaCapabilities?.decodingInfo(configuration) },
    mediaSource = globalThis.MediaSource || Reflect.get(globalThis, 'ManagedMediaSource'),
    signal = undefined,
    timeoutMs = 1500,
} = {}) {
    const output = directFallbackDescriptor(descriptor);
    if (!output || signal?.aborted || !Number.isSafeInteger(timeoutMs) || timeoutMs <= 0 || timeoutMs > 5000
        || typeof mediaCapabilities?.decodingInfo !== 'function' || typeof mediaSource?.isTypeSupported !== 'function') return 'unknown';
    let timer;
    let cancel;
    try {
        return await Promise.race([
            new Promise((resolve) => {
                cancel = () => resolve('unknown');
                signal?.addEventListener('abort', cancel, { once: true });
                timer = setTimeout(cancel, timeoutMs);
            }),
            Promise.resolve().then(async () => {
                const Hls = await loadHls();
                if (signal?.aborted) return 'unknown';
                if (!Hls?.isSupported?.() || !mediaSource.isTypeSupported(output.video.content_type)
                    || !mediaSource.isTypeSupported(output.audio.content_type)) return 'unsupported';
                const result = await mediaCapabilities.decodingInfo({
                    type: 'media-source',
                    video: { contentType: output.video.content_type, width: output.video.width, height: output.video.height, bitrate: output.video.bitrate, framerate: output.video.framerate },
                    audio: { contentType: output.audio.content_type, channels: output.audio.channels, bitrate: output.audio.bitrate, samplerate: output.audio.samplerate },
                });
                return signal?.aborted ? 'unknown' : result?.supported === true ? 'supported' : result?.supported === false ? 'unsupported' : 'unknown';
            }),
        ]);
    } catch { return 'unknown'; }
    finally { clearTimeout(timer); signal?.removeEventListener('abort', cancel); }
}
