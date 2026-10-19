// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { directFallbackDescriptor, directSourceRevision } from './capabilities.js';
import { boundedDecodingObservations, decodingAdmitted } from './decoding-capabilities.js';

export function copyFallbackDescriptor(value, generation) {
    const output = directFallbackDescriptor(value);
    return output && directSourceRevision(value.generation) && value.generation === generation
        ? Object.freeze({ generation: value.generation, ...output }) : null;
}

export async function probeCopyFallback(value, { signal = undefined, observe = boundedDecodingObservations } = {}) {
    const output = copyFallbackDescriptor(value, value?.generation);
    if (!output || signal?.aborted) return false;
    const observations = await observe([{
        type: 'media-source',
        video: { contentType: output.video.content_type, width: output.video.width, height: output.video.height, bitrate: output.video.bitrate, framerate: output.video.framerate },
        audio: { contentType: output.audio.content_type, channels: output.audio.channels, bitrate: output.audio.bitrate, samplerate: output.audio.samplerate },
    }], { signal });
    return !signal?.aborted && observations.length === 1 && decodingAdmitted(observations[0]);
}
