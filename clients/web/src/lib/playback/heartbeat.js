/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function createPlaybackHeartbeat({ readState, send, intervalMs = 15000 }) {
    if (typeof readState !== 'function' || typeof send !== 'function') throw new TypeError('Heartbeat requires state and transport functions.');
    if (!Number.isSafeInteger(intervalMs) || intervalMs <= 0 || intervalMs > 2147483647) throw new RangeError('Heartbeat interval must be a positive supported duration.');
    const inFlight = new Map();
    let timer = null;
    let timerRevision = 0;

    function sendNow() {
        const snapshot = readState();
        const sessionId = snapshot?.sessionId;
        if (!sessionId) return Promise.resolve();
        if (inFlight.has(sessionId)) return inFlight.get(sessionId);
        const body = Object.freeze({
            session_id: sessionId,
            position_ms: Math.floor(snapshot.positionMs),
            state: snapshot.isPlaying ? 'playing' : 'paused',
            is_buffering: snapshot.isBuffering,
        });
        const pending = Promise.resolve().then(() => send(body)).then(() => {}, () => {}).finally(() => {
            if (inFlight.get(sessionId) === pending) inFlight.delete(sessionId);
        });
        inFlight.set(sessionId, pending);
        return pending;
    }

    function stopTimer() {
        timerRevision += 1;
        if (timer !== null) clearInterval(timer);
        timer = null;
    }

    function start() {
        stopTimer();
        const revision = timerRevision;
        timer = setInterval(() => { if (revision === timerRevision) void sendNow(); }, intervalMs);
    }

    function drain(sessionId) {
        return inFlight.get(sessionId) || Promise.resolve();
    }

    return { start, stopTimer, sendNow, drain };
}
