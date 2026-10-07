/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { get } from 'svelte/store';
import { submitQoeReport } from '../api/quality.js';

export function createPlaybackTelemetry({ player, submit = submitQoeReport, intervalMs = 30_000 }) {
    let timer = null;
    let bufferStart = null;
    let disposed = false;

    async function report(extra = {}) {
        const state = get(player);
        if (disposed || !state.sessionId) return;
        try {
            await submit({ session_id: state.sessionId, position_ms: Math.floor(state.positionMs), is_playing: state.isPlaying, is_buffering: state.isBuffering, ...extra });
        } catch {}
    }

    function stop() {
        if (timer !== null) clearInterval(timer);
        timer = null;
        bufferStart = null;
    }

    return {
        start() { stop(); if (!disposed) timer = setInterval(() => report(), intervalMs); },
        waiting() { if (bufferStart === null) bufferStart = Date.now(); },
        playing() { if (bufferStart !== null) { const elapsed = Date.now() - bufferStart; bufferStart = null; report({ buffer_duration_ms: elapsed }); } },
        stop,
        dispose() { disposed = true; stop(); },
    };
}
