// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { get } from 'svelte/store';
import { createPlayerStore } from '../../src/lib/stores/player.js';
import { createMediaSource } from '../../src/lib/playback/source.js';

const video = document.querySelector('video');
const player = createPlayerStore();
const evidence = { errors: [], directFrames: 0, hlsFrames: 0, playbackBlocked: [] };
let attached = null;
let hlsAttachedAt = null;
const source = createMediaSource({
    video, shouldPlay: () => player.getPlayIntent(),
    onError: (error, details) => {
        evidence.errors.push({ code: Number(error?.code || 0), message: String(error?.message || '').slice(0, 256), position: details.positionSeconds,
            trusted: lastNativeErrorTrusted && lastNativeErrorCode === error?.code });
        void player.reportSourceFailure({ recoveryRequested: details.recoveryRequested, sourceUrl: details.url,
            sourceMode: details.mode, mediaErrorCode: details.mediaErrorCode,
            positionMs: (details.positionSeconds || 0) * 1000 + get(player).streamOffsetMs });
    },
    onPlaybackBlocked: (error) => evidence.playbackBlocked.push(error.name),
});
let lastNativeErrorTrusted = false;
let lastNativeErrorCode = null;
video.addEventListener('error', (event) => { lastNativeErrorTrusted = event.isTrusted; lastNativeErrorCode = video.error?.code ?? null; }, { capture: true });
const frames = () => {
    if (source.getTimeline().mode !== 'detached') {
        if (get(player).streamDecision === 'direct_play') evidence.directFrames += 1;
        else evidence.hlsFrames += 1;
    }
    video.requestVideoFrameCallback(frames);
};
video.requestVideoFrameCallback(frames);
video.addEventListener('timeupdate', () => {
    const state = get(player);
    if (!state.seeking && !state.isBuffering && source.getTimeline().mode !== 'detached') player.setPosition(Math.floor(video.currentTime * 1000 + state.streamOffsetMs));
});
video.addEventListener('loadedmetadata', () => {
    const state = get(player);
    if (state.streamDecision === 'direct_play' && state.positionMs > 0) video.currentTime = state.positionMs / 1000;
});
player.subscribe((state) => {
    if (state.streamUrl && state.streamUrl !== attached && !state.loading) {
        attached = state.streamUrl;
        hlsAttachedAt = state.streamDecision === 'transcode' ? performance.now() : null;
        void source.attach(attached);
    }
});
let audio;
let analyser;
document.querySelector('button').addEventListener('click', async () => {
    audio ||= new AudioContext({ sampleRate: 48000 });
    if (!analyser) {
        analyser = audio.createAnalyser();
        analyser.fftSize = 8192;
        analyser.smoothingTimeConstant = 0;
        const gain = audio.createGain();
        gain.gain.value = 0;
        audio.createMediaElementSource(video).connect(analyser).connect(gain).connect(audio.destination);
    }
    await audio.resume();
});
window.directFixture = {
    async start(fixture, candidate) {
        source.release();
        attached = null;
        await player.stop();
        evidence.errors = []; evidence.directFrames = 0; evidence.hlsFrames = 0;
        hlsAttachedAt = null;
        lastNativeErrorTrusted = false; lastNativeErrorCode = null;
        await player.setContext({ serverOrigin: location.origin, userId: fixture.userId, profileId: fixture.profileId });
        const response = await player.play({ id: candidate.item, runtime_seconds: 8 }, candidate.file, { audioStreamIndex: 2 });
        return { sessionId: response.session_id, decision: response.stream_decision };
    },
    snapshot() {
        const state = get(player);
        let peakHz = null;
        let peakDb = null;
        if (analyser) {
            const bins = new Float32Array(analyser.frequencyBinCount);
            analyser.getFloatFrequencyData(bins);
            let best = -Infinity;
            for (let index = 16; index < 350; index += 1) if (bins[index] > best) { best = bins[index]; peakHz = index * audio.sampleRate / analyser.fftSize; }
            peakDb = Number.isFinite(best) ? best : null;
        }
        return { ...evidence, sessionId: state.sessionId, decision: state.streamDecision, recovery: state.recovery, transcodeId: state.transcodeSessionId,
            positionMs: state.positionMs, offsetMs: state.streamOffsetMs, paused: video.paused, ready: video.readyState, width: video.videoWidth,
            peakHz, peakDb, audioRunning: audio?.state === 'running', hlsElapsedMs: hlsAttachedAt === null ? 0 : performance.now() - hlsAttachedAt };
    },
    async stop() { source.release(); attached = null; await player.stop(); },
};
