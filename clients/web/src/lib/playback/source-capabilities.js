// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { preparePlayback } from '../api/playback.js';
import { boundedDecodingObservations } from './decoding-capabilities.js';
import { playbackStartRequest } from './start-request.js';
import { capability } from './commands/schema.js';

const candidateIds = ['native_file', 'hls_video_copy_audio_copy', 'hls_video_copy_aac'];
const fingerprint = (value) => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
const positive = (value, maximum) => Number.isSafeInteger(value) && value > 0 && value <= maximum;
const keys = (value, allowed) => value && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).every((key) => allowed.includes(key));
const mime = (value, kind) => typeof value === 'string' && value.length <= 256
    && new RegExp(`^${kind}/[a-z0-9][a-z0-9.+-]*\\s*;\\s*codecs=(?:"[a-z0-9][a-z0-9._-]*"|[a-z0-9][a-z0-9._-]*)$`, 'i').test(value);

export function sourceRoute(value) {
    return value && candidateIds.includes(value.candidate_id) && fingerprint(value.source_revision)
        ? Object.freeze({ candidate_id: value.candidate_id, source_revision: value.source_revision }) : null;
}

function configuration(value, delivery) {
    const video = value?.video;
    const audio = value?.audio;
    if (value?.type !== (delivery === 'file' ? 'file' : 'media-source') || !video || !audio
        || !keys(value, ['type', 'video', 'audio'])
        || !keys(video, ['contentType', 'width', 'height', 'bitrate', 'framerate', 'colorGamut', 'transferFunction'])
        || !keys(audio, ['contentType', 'channels', 'bitrate', 'samplerate'])
        || !mime(video.contentType, 'video') || !positive(video.width, 16384) || !positive(video.height, 16384)
        || !positive(video.bitrate, 1000000000) || !Number.isFinite(video.framerate) || video.framerate <= 0 || video.framerate > 240
        || !mime(audio.contentType, 'audio') || typeof audio.channels !== 'string' || !/^(?:[1-9]|[12][0-9]|3[0-2])$/.test(audio.channels)
        || !positive(audio.bitrate, 20000000) || !positive(audio.samplerate, 384000)
        || video.transferFunction !== undefined && video.transferFunction !== 'srgb'
        || video.hdrMetadataType !== undefined || video.colorGamut !== undefined && !['srgb', 'p3', 'rec2020'].includes(video.colorGamut)) return null;
    return {
        type: value.type,
        video: { contentType: video.contentType, width: video.width, height: video.height, bitrate: video.bitrate, framerate: video.framerate,
            ...(video.colorGamut !== undefined ? { colorGamut: video.colorGamut } : {}),
            ...(video.transferFunction !== undefined ? { transferFunction: video.transferFunction } : {}) },
        audio: { contentType: audio.contentType, channels: audio.channels, bitrate: audio.bitrate, samplerate: audio.samplerate },
    };
}

export function sourcePreparation(value, { requiresHeaders = false, forceHls = false } = {}) {
    if (value?.protocol !== 1 || !fingerprint(value.source_revision) || !Array.isArray(value.candidates) || value.candidates.length > 3) return null;
    const seen = new Set();
    const candidates = [];
    for (const candidate of value.candidates) {
        if (!candidateIds.includes(candidate?.id) || seen.has(candidate.id) || candidate.delivery !== (candidate.id === 'native_file' ? 'file' : 'hls')
            || candidate.delivery === 'file' && (requiresHeaders || forceHls)) return null;
        const config = configuration(candidate.configuration, candidate.delivery);
        if (!config) return null;
        seen.add(candidate.id);
        candidates.push({ id: candidate.id, delivery: candidate.delivery, configuration: config });
    }
    return { protocol: 1, source_revision: value.source_revision, candidates };
}

export async function prepareSourceCapabilities(mediaItemId, mediaFileId, options = {}, {
    signal = undefined,
    requiresHeaders = false,
    prepare = preparePlayback,
    observe = boundedDecodingObservations,
    timeoutMs = 4000,
} = {}) {
    if (signal?.aborted) throw new DOMException('Playback request cancelled', 'AbortError');
    if (!Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 4000) return null;
    const controller = new AbortController();
    let timer;
    let abort;
    let timedOut = false;
    let commandProtocol = null;
    const deadline = new Promise((resolve, reject) => {
        abort = () => { reject(new DOMException('Playback request cancelled', 'AbortError')); controller.abort(); };
        signal?.addEventListener('abort', abort, { once: true });
        timer = setTimeout(() => { timedOut = true; resolve(null); controller.abort(); }, timeoutMs);
    });
    const run = async () => {
        const response = await prepare({ ...playbackStartRequest(mediaItemId, mediaFileId, options), capability_protocol: 1 }, { signal: controller.signal, cache: 'no-store', maxJsonBytes: 32768 });
        try { commandProtocol = capability(response?.command_capability); }
        catch { throw new Error('VOD_CAPABILITY_REFUSED'); }
        if (controller.signal.aborted) return null;
        const prepared = sourcePreparation(response, { requiresHeaders, forceHls: options.forceHls === true });
        if (!prepared) { if (commandProtocol) throw new Error('VOD_CAPABILITY_REFUSED'); return null; }
        const observations = await observe(prepared.candidates.map((candidate) => candidate.configuration), { signal: controller.signal });
        if (controller.signal.aborted || observations.length !== prepared.candidates.length) return null;
        const report = { version: 1, source_revision: prepared.source_revision, observations: observations.map((observation, index) => ({ candidate_id: prepared.candidates[index].id, ...observation })) };
        Object.defineProperty(report, 'commandProtocol', { value: commandProtocol });
        return report;
    };
    try { const result = await Promise.race([run(), deadline]); if (commandProtocol && !result) throw new Error('VOD_CAPABILITY_REFUSED'); return result; }
    catch (error) {
        if (commandProtocol || error?.message === 'VOD_CAPABILITY_REFUSED' || signal?.aborted || error?.name === 'AbortError' && !timedOut || error?.status === 401 || error?.status === 403) throw error;
        return null;
    } finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', abort);
        controller.abort();
    }
}
