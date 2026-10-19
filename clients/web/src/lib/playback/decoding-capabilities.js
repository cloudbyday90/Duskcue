// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function decodingObservation(status = 'unavailable') {
    return { status, supported: null, smooth: null, power_efficient: null, delivery_supported: null };
}

export function decodingAdmitted(value) {
    return value?.status === 'reported' && value.supported === true && value.smooth === true && value.delivery_supported === true;
}

export function createDecodingProbe({
    mediaCapabilities = { decodingInfo: (configuration) => globalThis.navigator?.mediaCapabilities?.decodingInfo(configuration) },
    mediaSource = globalThis.MediaSource || Reflect.get(globalThis, 'ManagedMediaSource'),
    createMediaElement = () => ({ canPlayType: (type) => globalThis.document?.createElement('video')?.canPlayType(type) }),
    loadHls = () => import('hls.js').then((module) => ({ isSupported: () => module.default.isSupported() })),
} = {}) {
    let hls;
    return async (configuration) => {
        const observation = decodingObservation();
        if (typeof mediaCapabilities?.decodingInfo !== 'function') return observation;
        const decoding = Promise.resolve().then(() => mediaCapabilities.decodingInfo(configuration)).then((result) => {
            if (!result) return;
            observation.status = 'reported';
            for (const [target, source] of [['supported', 'supported'], ['smooth', 'smooth'], ['power_efficient', 'powerEfficient']]) {
                observation[target] = typeof result[source] === 'boolean' ? result[source] : null;
            }
        }, () => { observation.status = 'rejected'; });
        const delivery = Promise.resolve().then(async () => {
            const types = [configuration.video?.contentType, configuration.audio?.contentType].filter(Boolean);
            if (configuration.type === 'media-source') {
                if (typeof mediaSource?.isTypeSupported !== 'function') return;
                if (!types.every((type) => mediaSource.isTypeSupported(type))) { observation.delivery_supported = false; return; }
                hls ||= Promise.resolve().then(loadHls);
                observation.delivery_supported = (await hls)?.isSupported?.() === true;
            } else {
                const media = createMediaElement();
                if (typeof media?.canPlayType === 'function') observation.delivery_supported = types.every((type) => Boolean(media.canPlayType(type)));
            }
        }).catch(() => {});
        await Promise.all([decoding, delivery]);
        return observation;
    };
}

export async function boundedDecodingObservations(configurations, {
    signal = undefined,
    timeoutMs = 1500,
    probe = createDecodingProbe(),
} = {}) {
    if (!Array.isArray(configurations) || configurations.length > 3 || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 1500) return [];
    const values = configurations.map(() => decodingObservation());
    if (signal?.aborted) return values;
    let timer;
    let cancel;
    const settled = configurations.map(() => false);
    try {
        await Promise.race([
            Promise.all(configurations.map(async (configuration, index) => {
                try { values[index] = await probe(configuration); }
                catch { values[index] = decodingObservation('rejected'); }
                settled[index] = true;
            })),
            new Promise((resolve) => {
                cancel = () => resolve(undefined);
                signal?.addEventListener('abort', cancel, { once: true });
                timer = setTimeout(cancel, timeoutMs);
            }),
        ]);
        return values.map((value, index) => settled[index] && !signal?.aborted ? { ...value } : decodingObservation(signal?.aborted ? 'unavailable' : 'timed_out'));
    } finally {
        clearTimeout(timer);
        signal?.removeEventListener('abort', cancel);
    }
}
