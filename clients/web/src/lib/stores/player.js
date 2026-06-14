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

function createPlayerStore() {
    let heartbeatTimer = null;

    const { subscribe, set, update } = writable({
        sessionId: null,
        mediaItem: null,
        mediaFileId: null,
        streamUrl: null,
        streamDecision: null,
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
    });

    function startHeartbeat() {
        stopHeartbeat();
        heartbeatTimer = setInterval(() => {
            sendHeartbeat();
        }, HEARTBEAT_INTERVAL_MS);
    }

    function stopHeartbeat() {
        if (heartbeatTimer !== null) {
            clearInterval(heartbeatTimer);
            heartbeatTimer = null;
        }
    }

    async function sendHeartbeat() {
        const state = get({ subscribe });
        if (!state.sessionId) return;
        try {
            await apiHeartbeat({
                session_id: state.sessionId,
                position_ms: Math.floor(state.positionMs),
                state: state.isPlaying ? 'playing' : 'paused',
                is_buffering: state.isBuffering,
            });
        } catch {
        }
    }

    return {
        subscribe,

        async play(mediaItem, mediaFileId, options = {}) {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiStartPlayback({
                    media_item_id: mediaItem.id,
                    media_file_id: mediaFileId,
                    device_profile: options.deviceProfile || null,
                    max_streaming_bitrate: options.maxBitrate || null,
                    force_transcode: options.forceTranscode || false,
                    quality_mode: options.qualityMode || 'auto',
                });

                const sessionId = result.session_id;
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
                    positionMs: options.startPositionMs || 0,
                    durationMs: (mediaItem.runtime_seconds || 0) * 1000,
                    isPlaying: true,
                    isBuffering: false,
                    loading: false,
                    error: null,
                }));

                startHeartbeat();
                return result;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async resume(sessionId) {
            try {
                const info = await apiGetPlaybackInfo(sessionId);
                update((s) => ({
                    ...s,
                    sessionId,
                    streamDecision: info.stream_decision || null,
                    transcodeSessionId: info.transcode_session_id || null,
                    positionMs: info.position_ms || 0,
                    isPlaying: true,
                    error: null,
                }));
                startHeartbeat();
                return info;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        setPlaying(playing) {
            update((s) => ({ ...s, isPlaying: playing, isBuffering: false }));
        },

        setBuffering(buffering) {
            update((s) => ({ ...s, isBuffering: buffering }));
        },

        setPosition(positionMs) {
            update((s) => ({ ...s, positionMs }));
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

        async seek(positionMs) {
            const state = get({ subscribe });
            if (!state.sessionId) return;

            update((s) => ({ ...s, positionMs, isBuffering: true }));

            try {
                const result = await apiSeek({
                    session_id: state.sessionId,
                    position_ms: Math.floor(positionMs),
                });

                if (result.stream_url || result.transcode_session_id) {
                    update((s) => ({
                        ...s,
                        transcodeSessionId: result.transcode_session_id || s.transcodeSessionId,
                        streamUrl: result.stream_url || s.streamUrl,
                        isBuffering: false,
                    }));
                } else {
                    update((s) => ({ ...s, isBuffering: false }));
                }

                return result;
            } catch {
                update((s) => ({ ...s, isBuffering: false }));
            }
        },

        async stop() {
            const state = get({ subscribe });
            stopHeartbeat();

            if (state.sessionId) {
                try {
                    await apiStopPlayback({
                        session_id: state.sessionId,
                        position_ms: Math.floor(state.positionMs),
                    });
                } catch {
                }
            }

            set({
                sessionId: null,
                mediaItem: null,
                mediaFileId: null,
                streamUrl: null,
                streamDecision: null,
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
            });
        },

        async sendHeartbeatNow() {
            await sendHeartbeat();
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
            stopHeartbeat();
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
