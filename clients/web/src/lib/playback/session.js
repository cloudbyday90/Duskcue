/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function createPlaybackSessionGuard(stopPlayback) {
    let generation = 0;
    const stops = new Map();
    return {
        begin: () => ++generation,
        isCurrent: (token) => token === generation,
        invalidate: () => ++generation,
        stop(sessionId, positionMs, cancelledBeforeStart = false, { contextKey = '', isCurrent = null } = {}) {
            if (!sessionId) return Promise.resolve();
            const key = JSON.stringify([contextKey, sessionId]);
            if (stops.has(key)) return stops.get(key);
            const promise = Promise.resolve().then(() => {
                if (isCurrent && !isCurrent()) throw new DOMException('Playback context changed', 'AbortError');
                return stopPlayback({
                    session_id: sessionId,
                    position_ms: Math.floor(Math.max(0, Number(positionMs) || 0)),
                    ...(cancelledBeforeStart ? { cancelled_before_start: true } : {}),
                });
            }).catch((error) => {
                if (stops.get(key) === promise) stops.delete(key);
                throw error;
            });
            stops.set(key, promise);
            return promise;
        },
    };
}
