// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export async function queryFormatCapability(configuration) {
    if (!configuration || !['file', 'media-source'].includes(configuration.type)
        || JSON.stringify(configuration).length > 8192) throw new Error('The format decoding configuration is invalid.');
    const output = { configuration, status: 'unavailable', supported: null, smooth: null, powerEfficient: null,
        mse: { video: null, audio: null }, error: null };
    const source = globalThis.MediaSource || Reflect.get(globalThis, 'ManagedMediaSource');
    if (typeof source?.isTypeSupported === 'function') {
        try {
            output.mse.video = source.isTypeSupported(configuration.video.contentType);
            output.mse.audio = source.isTypeSupported(configuration.audio.contentType);
        } catch (error) { output.error = String(error?.name || 'Error').slice(0, 80); }
    }
    if (typeof globalThis.navigator?.mediaCapabilities?.decodingInfo !== 'function') return output;
    let timer;
    try {
        const reply = await Promise.race([
            navigator.mediaCapabilities.decodingInfo(configuration).then((value) => ({ value })),
            new Promise((resolve) => { timer = setTimeout(() => resolve({ timedOut: true }), 1500); }),
        ]);
        if (reply.timedOut) output.status = 'timed_out';
        else {
            output.status = 'reported';
            for (const key of ['supported', 'smooth', 'powerEfficient']) output[key] = typeof reply.value?.[key] === 'boolean' ? reply.value[key] : null;
        }
    } catch (error) {
        output.status = 'rejected';
        output.error = String(error?.name || 'Error').slice(0, 80);
    } finally { clearTimeout(timer); }
    return output;
}
