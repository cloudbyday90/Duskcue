/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function createPlaybackHeartbeat({ readState, send, isCurrent = (_snapshot) => true, onResponse = (_response, _snapshot) => {}, onError = (_error, _snapshot) => {}, intervalMs = 15000 }) {
    if (typeof readState !== 'function' || typeof send !== 'function') throw new TypeError('Heartbeat requires state and transport functions.');
    if (!Number.isSafeInteger(intervalMs) || intervalMs <= 0 || intervalMs > 2147483647) throw new RangeError('Heartbeat interval must be a positive supported duration.');
    const inFlight = new Map();
    let timer = null;
    let timerRevision = 0;

    function sendNow() {
        const snapshot = Object.freeze({ ...readState() });
        const sessionId = snapshot?.sessionId;
        if (!sessionId) return Promise.resolve();
        const existing = inFlight.get(sessionId);
        if (existing) {
            if (snapshot.recoveryRequested && !existing.snapshot.recoveryRequested && !existing.followUp) {
                existing.followUp = existing.promise.then(() => sendNow());
            }
            return existing.followUp || existing.promise;
        }
        const body = Object.freeze({
            session_id: sessionId,
            position_ms: Math.floor(snapshot.positionMs),
            state: snapshot.isPlaying ? 'playing' : 'paused',
            is_buffering: snapshot.isBuffering,
            ...(snapshot.transcodeSessionId ? { transcode_session_id: snapshot.transcodeSessionId } : {}),
            ...(snapshot.directSourceRevision ? { direct_source_revision: snapshot.directSourceRevision } : {}),
            ...(snapshot.directFallbackSupported && snapshot.recoveryRequested ? { direct_fallback_supported: true } : {}),
            ...(snapshot.recoveryRequested && snapshot.recoveryResumePlaying === false ? { is_paused: true } : {}),
            ...(snapshot.recoveryRequested ? { recovery_requested: true } : {}),
        });
        const record = { snapshot, promise: null, followUp: null };
        const pending = Promise.resolve().then(() => isCurrent(snapshot) ? send(body) : undefined).then((response) => onResponse(response, snapshot), (error) => onError(error, snapshot)).then(() => {}, () => {}).finally(() => {
            if (inFlight.get(sessionId) === record) inFlight.delete(sessionId);
        });
        record.promise = pending;
        inFlight.set(sessionId, record);
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
        const record = inFlight.get(sessionId);
        return record?.followUp || record?.promise || Promise.resolve();
    }

    return { start, stopTimer, sendNow, drain };
}
