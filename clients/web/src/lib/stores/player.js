/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { writable, derived, get } from 'svelte/store';
import { createPlaybackSessionGuard } from '../playback/session.js';
import { createPlaybackHeartbeat } from '../playback/heartbeat.js';
import { createPlaybackRelease, playbackContextKey, PlaybackReleaseError } from '../playback/release.js';
import {
    startPlayback as apiStartPlayback,
    heartbeat as apiHeartbeat,
    stopPlayback as apiStopPlayback,
    seek as apiSeek,
    getPlaybackInfo as apiGetPlaybackInfo,
    streamFileUrl,
    transcodeManifestUrl,
} from '../api/playback.js';

const HEARTBEAT_INTERVAL_MS = 15000;
const VOLUME_STORAGE_KEY = 'duskcue_player_volume';

function loadVolume() {
    if (typeof localStorage === 'undefined') return 1;
    const stored = localStorage.getItem(VOLUME_STORAGE_KEY);
    if (stored === null) return 1;
    const vol = parseFloat(stored);
    return isNaN(vol) ? 1 : Math.max(0, Math.min(1, vol));
}

function saveVolume(volume) {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(VOLUME_STORAGE_KEY, String(volume));
}

export function createPlayerStore() {
    let pendingStop = null;
    let pendingPlay = null;
    let contextKey = null;
    let sessionContextKey = null;
    let releaseState = { phase: 'idle', error: null };
    const sessionGuard = createPlaybackSessionGuard(apiStopPlayback);

    function initialState() {
        return {
        sessionId: null,
        mediaItem: null,
        mediaFileId: null,
        streamUrl: null,
        streamDecision: null,
        streamOffsetMs: 0,
        transcodeSessionId: null,
        isPlaying: false,
        isBuffering: false,
        positionMs: 0,
        durationMs: 0,
        volume: loadVolume(),
        isMuted: false,
        isFullscreen: false,
        playbackRate: 1,
        error: null,
        loading: false,
        sessionReleased: false,
        release: releaseState,
        };
    }

    const { subscribe, set, update } = writable(initialState());
    const heartbeat = createPlaybackHeartbeat({ readState: () => {
        const state = get({ subscribe });
        return state.loading || state.sessionReleased || state.release.phase !== 'idle' ? { ...state, sessionId: null } : state;
    }, send: apiHeartbeat, intervalMs: HEARTBEAT_INTERVAL_MS });
    const release = createPlaybackRelease({
        readContext: () => contextKey,
        drain: heartbeat.drain,
        send: (record, isCurrent) => sessionGuard.stop(record.sessionId, record.positionMs, record.cancelledBeforeStart, { contextKey: record.contextKey, isCurrent }),
        onChange: (state) => {
            releaseState = state;
            update((current) => ({ ...current, release: state, ...(state.phase !== 'idle' ? { isPlaying: false, isBuffering: false } : {}) }));
        },
    });

    async function stopSession(state, options = {}) {
        const result = await release.request(state?.sessionId ? {
            sessionId: state.sessionId, positionMs: state.positionMs,
            cancelledBeforeStart: state.cancelledBeforeStart,
            contextKey: state.contextKey || sessionContextKey,
        } : null, options);
        if (state?.sessionId && get({ subscribe }).sessionId === state.sessionId) update((current) => ({ ...current, sessionReleased: true, isPlaying: false }));
        return result;
    }

    return {
        subscribe,

        play(mediaItem, mediaFileId, options = {}) {
            const precedingPlay = pendingPlay;
            const precedingStop = pendingStop;
            const operation = (async () => {
            if (options.signal?.aborted) return null;
            if (!contextKey) throw new PlaybackReleaseError('CONTEXT_REQUIRED');
            const ownedContext = contextKey;
            const token = sessionGuard.begin();
            const abort = () => {
                if (sessionGuard.isCurrent(token)) {
                    sessionGuard.invalidate();
                    update((s) => ({ ...s, loading: false }));
                }
            };
            options.signal?.addEventListener('abort', abort, { once: true });
            const previous = { ...get({ subscribe }), contextKey: sessionContextKey };
            heartbeat.stopTimer();
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                await precedingStop;
                await Promise.resolve(precedingPlay).catch(() => {});
                if (!sessionGuard.isCurrent(token) || contextKey !== ownedContext) return null;
                await stopSession(previous);
                if (!sessionGuard.isCurrent(token)) return null;
                const result = await apiStartPlayback({
                    media_item_id: mediaItem.id,
                    media_file_id: mediaFileId,
                    device_profile: options.deviceProfile || null,
                    max_streaming_bitrate: options.maxBitrate || null,
                    force_transcode: options.forceTranscode || false,
                    quality_mode: options.qualityMode || 'auto',
                    audio_stream_index: options.audioStreamIndex ?? null,
                    subtitle_stream_index: options.subtitleStreamIndex ?? null,
                });

                if (!sessionGuard.isCurrent(token)) {
                    if (contextKey === ownedContext) await stopSession({ sessionId: result.session_id, positionMs: 0, cancelledBeforeStart: true, contextKey: ownedContext });
                    return null;
                }

                const sessionId = result.session_id;
                sessionContextKey = ownedContext;
                let streamUrl;
                const decision = result.stream_decision || result.playback_type || 'direct_play';

                if (decision === 'transcode' || decision === 'direct_stream') {
                    const tsId = result.transcode_session_id;
                    streamUrl = transcodeManifestUrl(tsId);
                    update((s) => ({
                        ...s,
                        transcodeSessionId: tsId,
                    }));
                } else {
                    streamUrl = streamFileUrl(mediaFileId);
                }

                update((s) => ({
                    ...s,
                    sessionId,
                    mediaItem,
                    mediaFileId,
                    streamUrl,
                    streamDecision: decision,
                    streamOffsetMs: 0,
                    positionMs: options.startPositionMs || 0,
                    durationMs: (mediaItem.runtime_seconds || 0) * 1000,
                    isPlaying: true,
                    isBuffering: false,
                    loading: false,
                    error: null,
                    sessionReleased: false,
                }));

                heartbeat.start();
                return result;
            } catch (err) {
                if (options.signal?.aborted) return null;
                if (sessionGuard.isCurrent(token)) update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            } finally {
                options.signal?.removeEventListener('abort', abort);
            }
            })();
            pendingPlay = operation;
            const clear = () => { if (pendingPlay === operation) pendingPlay = null; };
            operation.then(clear, clear);
            return operation;
        },

        async resume(sessionId) {
            if (!contextKey) throw new PlaybackReleaseError('CONTEXT_REQUIRED');
            const ownedContext = contextKey;
            const token = sessionGuard.begin();
            try {
                if (release.hasPending()) await release.request();
                const info = await apiGetPlaybackInfo(sessionId);
                if (!sessionGuard.isCurrent(token) || contextKey !== ownedContext) return null;
                sessionContextKey = ownedContext;
                update((s) => ({
                    ...s,
                    sessionId,
                    streamDecision: info.stream_decision || null,
                    transcodeSessionId: info.transcode_session_id || null,
                    positionMs: info.position_ms || 0,
                    isPlaying: true,
                    error: null,
                    sessionReleased: false,
                }));
                heartbeat.start();
                return info;
            } catch (err) {
                if (sessionGuard.isCurrent(token)) update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        setPlaying(playing) {
            update((s) => ({ ...s, isPlaying: playing && !s.sessionReleased && s.release.phase === 'idle', isBuffering: false }));
        },

        setBuffering(buffering) {
            update((s) => ({ ...s, isBuffering: buffering }));
        },

        setPosition(positionMs) {
            update((s) => s.sessionReleased || s.release.phase !== 'idle' ? s : ({ ...s, positionMs }));
        },

        setDuration(durationMs) {
            update((s) => ({ ...s, durationMs }));
        },

        setVolume(volume) {
            const clamped = Math.max(0, Math.min(1, volume));
            saveVolume(clamped);
            update((s) => ({ ...s, volume: clamped, isMuted: clamped === 0 }));
        },

        toggleMute() {
            update((s) => ({ ...s, isMuted: !s.isMuted }));
        },

        setPlaybackRate(rate) {
            update((s) => ({ ...s, playbackRate: rate }));
        },

        toggleFullscreen() {
            update((s) => ({ ...s, isFullscreen: !s.isFullscreen }));
        },

        setFullscreen(fullscreen) {
            update((s) => ({ ...s, isFullscreen: fullscreen }));
        },

        async seek(positionMs, options = {}) {
            if (options.signal?.aborted) return null;
            const state = get({ subscribe });
            if (!state.sessionId || state.sessionReleased || state.release.phase !== 'idle') return null;
            const token = sessionGuard.begin();
            const abort = () => {
                if (sessionGuard.isCurrent(token)) {
                    sessionGuard.invalidate();
                    update((s) => ({ ...s, isBuffering: false }));
                }
            };
            options.signal?.addEventListener('abort', abort, { once: true });

            update((s) => ({ ...s, positionMs, isBuffering: true }));

            try {
                const result = await apiSeek({
                    session_id: state.sessionId,
                    position_ms: Math.floor(positionMs),
                });
                if (!sessionGuard.isCurrent(token)) return null;

                if (result.stream_url || result.transcode_session_id) {
                    update((s) => ({
                        ...s,
                        transcodeSessionId: result.transcode_session_id || s.transcodeSessionId,
                        streamUrl: result.transcode_session_id ? transcodeManifestUrl(result.transcode_session_id) : result.stream_url || s.streamUrl,
                        streamOffsetMs: positionMs,
                        isBuffering: false,
                    }));
                } else {
                    update((s) => ({ ...s, isBuffering: false }));
                }

                return result;
            } catch (error) {
                if (options.signal?.aborted) return null;
                if (sessionGuard.isCurrent(token)) update((s) => ({ ...s, isBuffering: false }));
                throw error;
            } finally {
                options.signal?.removeEventListener('abort', abort);
            }
        },

        stop({ retry = false } = {}) {
            if (pendingStop) return pendingStop;
            const state = { ...get({ subscribe }), contextKey: sessionContextKey };
            const token = sessionGuard.invalidate();
            const ownedContext = contextKey;
            const starting = pendingPlay;
            heartbeat.stopTimer();
            update((current) => ({ ...current, isPlaying: false, isBuffering: false, loading: false }));
            const operation = Promise.resolve(starting).catch(() => {}).then(async () => {
                if (contextKey !== ownedContext) throw new DOMException('Playback context changed', 'AbortError');
                await stopSession(state, { retry });
                if (contextKey === ownedContext && sessionGuard.isCurrent(token)) {
                    sessionContextKey = null;
                    set(initialState());
                }
            }).catch((error) => {
                if (contextKey === ownedContext && (sessionGuard.isCurrent(token) || release.getState().phase === 'failed')) update((current) => ({ ...current, error }));
                throw error;
            }).finally(() => { if (pendingStop === operation) pendingStop = null; });
            pendingStop = operation;
            return operation;
        },

        async retryRelease() {
            heartbeat.stopTimer();
            await stopSession(get({ subscribe }), { retry: true });
        },

        setContext(context) {
            const next = playbackContextKey(context);
            if (next === contextKey) return;
            contextKey = next;
            sessionContextKey = null;
            sessionGuard.invalidate();
            heartbeat.stopTimer();
            release.invalidate();
            pendingStop = null;
            pendingPlay = null;
            set(initialState());
        },

        reset({ preserveRelease = false } = {}) {
            sessionGuard.invalidate();
            heartbeat.stopTimer();
            if (!preserveRelease) release.invalidate();
            sessionContextKey = null;
            set(initialState());
        },

        async sendHeartbeatNow() {
            await heartbeat.sendNow();
        },

        getStreamUrl() {
            let url = null;
            const unsub = subscribe((s) => {
                url = s.streamUrl;
            });
            unsub();
            return url;
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },

        destroy() {
            heartbeat.stopTimer();
        },
    };
}

export const player = createPlayerStore();

export const isPlaying = derived(player, ($player) => $player.isPlaying);

export const isBuffering = derived(player, ($player) => $player.isBuffering);

export const currentPosition = derived(player, ($player) => $player.positionMs);

export const currentDuration = derived(player, ($player) => $player.durationMs);

export const streamUrl = derived(player, ($player) => $player.streamUrl);

export const streamDecision = derived(player, ($player) => $player.streamDecision);

export const currentMediaItem = derived(player, ($player) => $player.mediaItem);

export const playerVolume = derived(player, ($player) => $player.volume);

export const playerError = derived(player, ($player) => $player.error);

export const playerLoading = derived(player, ($player) => $player.loading);

export const progressPercent = derived(
    player,
    ($player) =>
        $player.durationMs > 0
            ? Math.min(100, ($player.positionMs / $player.durationMs) * 100)
            : 0,
);
