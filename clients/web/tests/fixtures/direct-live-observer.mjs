// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { get } from 'svelte/store';

export function createDirectLiveObserver({ player, root }) {
    let video = null;
    let frame = null;
    let audio = null;
    let analyser = null;
    let input = null;
    let attachedGeneration = null;
    const sourceGenerations = new Map();
    let generations = [];
    let errors = [];
    let directFrames = 0;
    let hlsFrames = 0;

    const generation = () => get(player).transcodeSessionId;
    const loaded = () => {
        const source = video?.currentSrc;
        const id = generation();
        const known = sourceGenerations.get(source);
        if (!source?.startsWith('blob:') || !id || (known && known !== id) || (!known && sourceGenerations.size >= 4)) {
            attachedGeneration = null;
            return;
        }
        sourceGenerations.set(source, id);
        attachedGeneration = id;
    };
    const emptied = () => { attachedGeneration = null; };
    const failed = (event) => {
        if (!(event.target instanceof HTMLVideoElement) || errors.length >= 16) return;
        const media = event.target;
        errors.push({ code: media.error?.code ?? null, position: Number.isFinite(media.currentTime) ? media.currentTime : null,
            trusted: event.isTrusted, generation: generation(), decision: get(player).streamDecision });
    };
    root.addEventListener('error', failed, true);

    function presented(now, metadata) {
        const state = get(player);
        if (video?.readyState >= 2 && state.streamDecision === 'direct_play'
            && video.currentSrc === new URL(state.streamUrl, location.origin).href) directFrames += 1;
        else if (video?.readyState >= 2 && state.transcodeSessionId && attachedGeneration === state.transcodeSessionId && video.currentSrc.startsWith('blob:')) {
            let record = generations.find((item) => item.id === state.transcodeSessionId);
            if (!record && generations.length < 4) {
                record = { id: state.transcodeSessionId, firstMediaTime: metadata.mediaTime, firstPresentedAt: now, frames: 0 };
                generations.push(record);
            }
            if (record) { record.frames += 1; record.lastMediaTime = metadata.mediaTime; record.lastPresentedAt = now; }
            hlsFrames += 1;
        }
        if (video) frame = video.requestVideoFrameCallback(presented);
    }

    function connectAudio() {
        if (!video || !audio || !analyser || input) return;
        input = audio.createMediaElementSource(video);
        input.connect(analyser);
    }

    function detach() {
        if (frame !== null && video) video.cancelVideoFrameCallback(frame);
        frame = null;
        video?.removeEventListener('loadstart', loaded);
        video?.removeEventListener('loadedmetadata', loaded);
        video?.removeEventListener('loadeddata', loaded);
        video?.removeEventListener('canplay', loaded);
        video?.removeEventListener('emptied', emptied);
        input?.disconnect(); input = null;
        video = null; attachedGeneration = null;
    }

    return {
        async enableAudio() {
            audio ||= new AudioContext({ sampleRate: 48000 });
            if (!analyser) {
                analyser = audio.createAnalyser(); analyser.fftSize = 8192; analyser.smoothingTimeConstant = 0;
                const gain = audio.createGain(); gain.gain.value = 0;
                analyser.connect(gain).connect(audio.destination);
            }
            connectAudio();
            await audio.resume();
        },
        attach(media) {
            detach(); video = media;
            video.addEventListener('loadstart', loaded);
            video.addEventListener('loadedmetadata', loaded);
            video.addEventListener('loadeddata', loaded);
            video.addEventListener('canplay', loaded);
            video.addEventListener('emptied', emptied);
            loaded(); connectAudio();
            frame = video.requestVideoFrameCallback(presented);
        },
        detach,
        reset() { errors = []; generations = []; sourceGenerations.clear(); directFrames = 0; hlsFrames = 0; },
        snapshot() {
            let peakHz = null;
            let peakDb = null;
            if (analyser) {
                const bins = new Float32Array(analyser.frequencyBinCount);
                analyser.getFloatFrequencyData(bins);
                let best = -Infinity;
                for (let index = 16; index < 350; index += 1) if (bins[index] > best) { best = bins[index]; peakHz = index * audio.sampleRate / analyser.fftSize; }
                peakDb = Number.isFinite(best) ? best : null;
            }
            const current = generations.find((item) => item.id === generation());
            const notice = root.querySelector('.recovery-status');
            const box = notice?.getBoundingClientRect();
            const style = notice && getComputedStyle(notice);
            const visible = !!box && box.width > 0 && box.height > 0 && style.display !== 'none' && style.visibility !== 'hidden' && Number(style.opacity) > 0;
            const text = notice?.textContent?.trim() ?? '';
            return { errors: errors.map((error) => ({ ...error })), directFrames, hlsFrames,
                generations: generations.map((item) => ({ ...item })),
                paused: video?.paused ?? true, ready: video?.readyState ?? 0, width: video?.videoWidth ?? 0,
                mediaTime: video?.currentTime ?? null, currentSrc: video?.currentSrc ?? null,
                generationFrames: current?.frames ?? 0, firstFrameMediaTime: current?.firstMediaTime ?? null,
                frameMediaTime: current?.lastMediaTime ?? null, framePresentedAt: current?.lastPresentedAt ?? null,
                peakHz, peakDb, audioRunning: audio?.state === 'running',
                hlsElapsedMs: current ? performance.now() - current.firstPresentedAt : 0,
                recoveryNotice: text, recoveryNoticeVisible: visible, visibleNotice: visible ? text : '' };
        },
    };
}
