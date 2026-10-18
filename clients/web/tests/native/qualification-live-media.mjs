// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';

export async function observeNativeAudio(page, video, expectedHz, { attachedAt = null, minimumElapsedMs = 0 } = {}) {
    if (!Number.isFinite(expectedHz) || expectedHz < 100 || expectedHz > 10000) throw new Error('Native fixture audio identity is required.');
    if (!Number.isFinite(minimumElapsedMs) || minimumElapsedMs < 0 || minimumElapsedMs > 10000
        || attachedAt !== null && (!Number.isFinite(attachedAt) || attachedAt < 0)) throw new Error('Native audio observation timing is invalid.');
    await video.evaluate(async (element) => {
        if (!window.nativeLiveAudio) {
            const context = new AudioContext({ sampleRate: 48000 });
            const analyser = context.createAnalyser();
            analyser.fftSize = 8192;
            analyser.smoothingTimeConstant = 0;
            const gain = context.createGain();
            gain.gain.value = 0;
            context.createMediaElementSource(element).connect(analyser).connect(gain).connect(context.destination);
            window.nativeLiveAudio = { context, analyser, video: element };
        }
        if (window.nativeLiveAudio.video !== element) throw new Error('Native audio observation lost its video owner.');
        await window.nativeLiveAudio.context.resume();
    });
    const sample = () => page.evaluate((attachedAt) => {
        const { context, analyser } = window.nativeLiveAudio;
        const bins = new Float32Array(analyser.frequencyBinCount);
        analyser.getFloatFrequencyData(bins);
        let level = -Infinity;
        let frequency = 0;
        for (let index = 16; index < 1700; index += 1) {
            if (bins[index] > level) { level = bins[index]; frequency = index * context.sampleRate / analyser.fftSize; }
        }
        return { frequency, level: Number.isFinite(level) ? level : null, running: context.state === 'running',
            elapsedMs: attachedAt === null ? null : performance.now() - attachedAt };
    }, attachedAt);
    await expect.poll(async () => {
        const audio = await sample();
        return audio.running && audio.level > -60 && Math.abs(audio.frequency - expectedHz) < 20
            && (minimumElapsedMs === 0 || audio.elapsedMs >= minimumElapsedMs);
    }, { timeout: 10000 }).toBe(true);
    return sample();
}

export async function releaseNativeAudio(page) {
    await page.evaluate(async () => {
        if (window.nativeLiveAudio) await window.nativeLiveAudio.context.close();
        delete window.nativeLiveAudio;
    });
}
