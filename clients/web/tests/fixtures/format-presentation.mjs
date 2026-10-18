// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function installFormatPresentation() {
    if (window.formatPresentation) throw new Error('A format presentation observer is already attached.');
    const video = document.querySelector('video.player-video');
    if (!(video instanceof HTMLVideoElement) || typeof video.requestVideoFrameCallback !== 'function') throw new Error('Actual format presentation callbacks are unavailable.');
    const sources = new Map();
    let owned = null;
    let frame = null;
    let disposed = false;
    const invalidate = () => { owned = null; };
    const loaded = () => { owned = sources.get(video.currentSrc) || null; };
    const presented = (now, metadata) => {
        if (owned && video.currentSrc === owned.source && video.readyState >= 2) {
            if (owned.frames === 0) { owned.firstMediaTime = metadata.mediaTime; owned.firstPresentedAt = now; }
            owned.frames += 1;
            owned.lastMediaTime = metadata.mediaTime;
            owned.lastPresentedAt = now;
        }
        if (!disposed) frame = video.requestVideoFrameCallback(presented);
    };
    video.addEventListener('emptied', invalidate);
    video.addEventListener('loadstart', invalidate);
    for (const event of ['loadedmetadata', 'loadeddata', 'canplay']) video.addEventListener(event, loaded);
    frame = video.requestVideoFrameCallback(presented);
    window.formatPresentation = {
        bind(generation) {
            const source = video.currentSrc;
            if (!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(generation || '') || !source.startsWith('blob:')
                || sources.has(source) && sources.get(source).generation !== generation || sources.size >= 3) throw new Error('Format presentation lost its canonical source generation.');
            owned = { generation, source, frames: 0, firstMediaTime: null, lastMediaTime: null, firstPresentedAt: null, lastPresentedAt: null, attachedAt: performance.now() };
            sources.set(source, owned);
            return { generation, attachedAt: owned.attachedAt };
        },
        snapshot() {
            return { ...(owned || {}), paused: video.paused, ready: video.readyState, currentTime: video.currentTime,
                width: video.videoWidth, height: video.videoHeight, currentSource: video.currentSrc };
        },
        detach() {
            disposed = true;
            if (frame !== null) video.cancelVideoFrameCallback(frame);
            video.removeEventListener('emptied', invalidate);
            video.removeEventListener('loadstart', invalidate);
            for (const event of ['loadedmetadata', 'loadeddata', 'canplay']) video.removeEventListener(event, loaded);
            delete window.formatPresentation;
        },
    };
}
