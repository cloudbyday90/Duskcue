// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function installSourcePresentation() {
    if (window.sourcePresentation) throw new Error('A source presentation observer is already attached.');
    const video = document.querySelector('video.player-video');
    if (!(video instanceof HTMLVideoElement) || typeof video.requestVideoFrameCallback !== 'function') throw new Error('Actual source presentation callbacks are unavailable.');
    let owned = null;
    const sources = new Map();
    let frame = null;
    let disposed = false;
    const invalidate = () => { owned = null; };
    const record = (source) => ({ owner: null, native: null, source, frames: 0, firstMediaTime: null, lastMediaTime: null,
        firstPresentedAt: null, lastPresentedAt: null, attachedAt: performance.now() });
    const seeking = () => {
        const source = video.currentSrc;
        if (source && !source.startsWith('blob:')) { sources.set(source, record(source)); invalidate(); }
    };
    const presented = (now, metadata) => {
        const source = video.currentSrc;
        if (source && video.readyState >= 2) {
            if (!sources.has(source) && sources.size < 6) sources.set(source, record(source));
            const current = sources.get(source);
            if (current) {
                if (!current.frames) { current.firstMediaTime = metadata.mediaTime; current.firstPresentedAt = now; }
                current.frames += 1;
                current.lastMediaTime = metadata.mediaTime;
                current.lastPresentedAt = now;
            }
        }
        if (!disposed) frame = video.requestVideoFrameCallback(presented);
    };
    for (const event of ['emptied', 'loadstart']) video.addEventListener(event, invalidate);
    video.addEventListener('seeking', seeking);
    frame = video.requestVideoFrameCallback(presented);
    window.sourcePresentation = {
        bind({ owner, native, fileId }) {
            const source = video.currentSrc;
            if (!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(owner || '')
                || (native ? source !== new URL(`/api/v1/stream/${fileId}`, location.origin).href : !source.startsWith('blob:'))) throw new Error('Source presentation lost its canonical source owner.');
            if (!sources.has(source)) {
                if (sources.size >= 6) throw new Error('Source presentation exceeded its source bound.');
                sources.set(source, record(source));
            }
            owned = sources.get(source);
            if (owned.owner && owned.owner !== owner) throw new Error('Source presentation replayed an obsolete source owner.');
            owned.owner = owner;
            owned.native = native;
            return { owner, source, attachedAt: owned.attachedAt };
        },
        snapshot() { return { ...(owned || {}), ready: video.readyState, width: video.videoWidth, height: video.videoHeight,
            currentSource: video.currentSrc, paused: video.paused, ended: video.ended, currentTime: video.currentTime }; },
        detach() {
            disposed = true;
            if (frame !== null) video.cancelVideoFrameCallback(frame);
            for (const event of ['emptied', 'loadstart']) video.removeEventListener(event, invalidate);
            video.removeEventListener('seeking', seeking);
            delete window.sourcePresentation;
        },
    };
}
