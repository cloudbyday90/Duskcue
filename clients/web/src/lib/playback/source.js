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

import { mediaRequestHeaders, requiresAuthenticatedMediaSource } from '../api/core.js';
import { onDemandHlsTiming, sourceTimeline } from './timing.js';
import { createHlsMediaRecovery } from './hls-media-recovery.js';

export function createMediaSource({ video, loadHls = null, shouldPlay = () => true, onError = (_error, _details) => {}, onPlaybackBlocked = (_error) => {}, onTimelineChange = (_timeline) => {}, requestHeaders = mediaRequestHeaders, requiresHeaders = requiresAuthenticatedMediaSource }) {
    const resolveHls = loadHls || (() => import('hls.js').then((module) => module.default));
    let generation = 0;
    let hls = null;
    let hlsRecovery = null;
    let disposed = false;
    let attachedUrl = null;
    let metadataListener = null;
    let timeline = sourceTimeline({ generation });

    function publishTimeline(mode, details = null) {
        timeline = sourceTimeline({ generation, url: attachedUrl, mode, details });
        if (!disposed) onTimelineChange(timeline);
    }

    const mediaError = () => {
        if (disposed || !attachedUrl || !video.error || video.error.code === 1) return;
        if (hls) {
            if (video.error.code === 3) hlsRecovery?.report(video.error, 'native');
        } else fail(video.error, video.error.code === 3, true);
    };
    video.addEventListener?.('error', mediaError);

    function fail(error, recoveryRequested = false, detach = false, positionSeconds = null) {
        const details = { url: attachedUrl, mode: timeline.mode, mediaErrorCode: Number.isInteger(error?.code) ? error.code : null,
            positionSeconds: positionSeconds ?? (Number.isFinite(video.currentTime) ? Math.max(0, video.currentTime) : null), recoveryRequested };
        try { onError(error, details); } finally { if (detach) release(); }
    }

    function play(current, autoplay = true) {
        if (!autoplay || !shouldPlay()) return;
        video.play().catch((error) => { if (current()) { if (video.error && video.error.code !== 1) { if (!hls) fail(video.error, video.error.code === 3, true); } else onPlaybackBlocked(error); } });
    }

    function release() {
        if (metadataListener) video.removeEventListener?.('loadedmetadata', metadataListener);
        metadataListener = null;
        generation += 1;
        attachedUrl = null;
        hlsRecovery?.dispose();
        hlsRecovery = null;
        const engine = hls;
        hls = null;
        engine?.destroy();
        publishTimeline('detached');
        video.pause();
        video.removeAttribute('src');
        video.load();
    }

    async function attach(url, { autoplay = true, startPosition = 0 } = {}) {
        if (disposed) return;
        release();
        const token = generation;
        const current = () => !disposed && generation === token;
        if (!url) return;
        attachedUrl = url;
        try {
            if (!Number.isFinite(startPosition) || startPosition < 0 || startPosition > 2147483.647) throw new RangeError('Playback attachment position is invalid');
            if (url.includes('.m3u8')) {
                publishTimeline('hls-pending');
                const Hls = await resolveHls();
                if (!current()) return;
                if (Hls.isSupported()) {
                    const engine = new Hls({ ...onDemandHlsTiming, startPosition, enableWorker: true, lowLatencyMode: false, backBufferLength: 90, xhrSetup: (xhr, requestUrl) => {
                        for (const [name, value] of Object.entries(requestHeaders(requestUrl))) xhr.setRequestHeader(name, value);
                    } });
                    hls = engine;
                    const ownsEngine = () => current() && hls === engine;
                    publishTimeline('hls-mse');
                    let networkRecoveries = 0;
                    hlsRecovery = createHlsMediaRecovery({ video, isCurrent: ownsEngine, retry: () => engine.recoverMediaError(),
                        resume: () => play(ownsEngine), onFailure: (error, seconds) => fail(error, true, true, seconds) });
                    engine.on(Hls.Events.MANIFEST_PARSED, () => { if (ownsEngine()) play(ownsEngine, autoplay); });
                    engine.on(Hls.Events.LEVEL_UPDATED, (_event, data) => {
                        if (ownsEngine()) publishTimeline('hls-mse', data.details);
                    });
                    engine.on(Hls.Events.ERROR, (_event, data) => {
                        if (!ownsEngine() || !data.fatal) return;
                        const status = Number(data.response?.code || 0);
                        const denied = status >= 400 && status < 500 && ![408, 429].includes(status);
                        if (data.type === Hls.ErrorTypes.NETWORK_ERROR && !denied && networkRecoveries++ < 1) engine.startLoad();
                        else if (data.type === Hls.ErrorTypes.MEDIA_ERROR && !denied) hlsRecovery.report(data, 'hls');
                        else fail(data, data.type === Hls.ErrorTypes.MEDIA_ERROR && !denied, true);
                    });
                    engine.loadSource(url);
                    engine.attachMedia(video);
                    return;
                }
                if (requiresHeaders() || !video.canPlayType('application/vnd.apple.mpegurl')) throw new Error('Authenticated HLS is unavailable');
            }
            if (requiresHeaders()) throw new Error('Authenticated playback requires HLS');
            if (!current()) return;
            publishTimeline(url.includes('.m3u8') ? 'native-hls' : 'direct');
            if (startPosition > 0) {
                metadataListener = () => {
                    video.removeEventListener?.('loadedmetadata', metadataListener);
                    metadataListener = null;
                    if (!current()) return;
                    try { video.currentTime = startPosition; play(current, autoplay); }
                    catch (error) { fail(error, false, true); }
                };
                video.addEventListener?.('loadedmetadata', metadataListener);
            }
            video.src = url;
            if (startPosition === 0) play(current, autoplay);
        } catch (error) { if (current()) fail(error); }
    }

    return { attach, release, getTimeline() { return timeline; }, dispose() { disposed = true; video.removeEventListener?.('error', mediaError); release(); } };
}
