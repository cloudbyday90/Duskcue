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

export const onDemandHlsTiming = Object.freeze({ startPosition: 0, liveSyncDuration: Infinity });

function positive(value) {
    return Number.isFinite(value) && value > 0 ? value : 0;
}

export function sourceTimeline({ generation, url = null, mode = 'detached', details = null }) {
    return Object.freeze({
        generation, url, mode,
        playlistType: typeof details?.type === 'string' ? details.type : null,
        complete: mode === 'direct' ? true : mode === 'hls-mse' && typeof details?.live === 'boolean' ? !details.live : null,
        producedDurationMs: positive(details?.totalduration) * 1000,
        streamEndMs: positive(details?.edge) * 1000,
    });
}

export function sourcePlaybackDuration({ source, mediaDurationSeconds, runtimeSeconds, streamOffsetMs = 0, naturalEnded = false }) {
    const knownDuration = positive(runtimeSeconds) * 1000;
    const offset = positive(streamOffsetMs);
    const mediaDuration = positive(mediaDurationSeconds) * 1000;
    const incomplete = source?.mode === 'hls-pending' || source?.mode === 'hls-mse' && source.complete !== true
        || source?.mode === 'native-hls' && !naturalEnded;
    if (incomplete && knownDuration) return knownDuration;
    const finalEnd = source?.mode === 'hls-mse' && source.complete === true ? positive(source.streamEndMs) : 0;
    const relativeDuration = Math.max(mediaDuration, finalEnd);
    return relativeDuration ? relativeDuration + offset : knownDuration;
}

export function canCompleteSource(source) {
    return source?.mode === 'direct' || source?.mode === 'native-hls' || source?.mode === 'hls-mse' && source.complete === true;
}
