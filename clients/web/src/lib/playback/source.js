import { mediaRequestHeaders, requiresAuthenticatedMediaSource } from '../api/core.js';

export function createMediaSource({ video, loadHls = null, onError = (_error) => {}, onPlaybackBlocked = (_error) => {}, requestHeaders = mediaRequestHeaders, requiresHeaders = requiresAuthenticatedMediaSource }) {
    const resolveHls = loadHls || (() => import('hls.js').then((module) => module.default));
    let generation = 0;
    let hls = null;
    let disposed = false;
    let attachedUrl = null;

    const mediaError = () => {
        if (!disposed && attachedUrl && video.error && video.error.code !== 1) onError(video.error);
    };
    video.addEventListener?.('error', mediaError);

    function play(current) {
        video.play().catch((error) => { if (current()) { if (video.error && video.error.code !== 1) onError(video.error); else onPlaybackBlocked(error); } });
    }

    function release() {
        generation += 1;
        attachedUrl = null;
        hls?.destroy();
        hls = null;
        video.pause();
        video.removeAttribute('src');
        video.load();
    }

    async function attach(url) {
        if (disposed) return;
        release();
        const token = generation;
        const current = () => !disposed && generation === token;
        if (!url) return;
        attachedUrl = url;
        try {
            if (url.includes('.m3u8')) {
                const Hls = await resolveHls();
                if (!current()) return;
                if (Hls.isSupported()) {
                    const engine = new Hls({ enableWorker: true, lowLatencyMode: false, backBufferLength: 90, xhrSetup: (xhr, requestUrl) => {
                        for (const [name, value] of Object.entries(requestHeaders(requestUrl))) xhr.setRequestHeader(name, value);
                    } });
                    hls = engine;
                    let networkRecoveries = 0;
                    let mediaRecoveries = 0;
                    engine.on(Hls.Events.MANIFEST_PARSED, () => { if (current()) play(current); });
                    engine.on(Hls.Events.ERROR, (_event, data) => {
                        if (!current() || !data.fatal) return;
                        const status = Number(data.response?.code || 0);
                        const denied = status >= 400 && status < 500 && ![408, 429].includes(status);
                        if (data.type === Hls.ErrorTypes.NETWORK_ERROR && !denied && networkRecoveries++ < 1) engine.startLoad();
                        else if (data.type === Hls.ErrorTypes.MEDIA_ERROR && mediaRecoveries++ < 1) engine.recoverMediaError();
                        else { release(); onError(data); }
                    });
                    engine.loadSource(url);
                    engine.attachMedia(video);
                    return;
                }
                if (requiresHeaders() || !video.canPlayType('application/vnd.apple.mpegurl')) throw new Error('Authenticated HLS is unavailable');
            }
            if (requiresHeaders()) throw new Error('Authenticated playback requires HLS');
            if (!current()) return;
            video.src = url;
            play(current);
        } catch (error) { if (current()) onError(error); }
    }

    return { attach, release, dispose() { disposed = true; video.removeEventListener?.('error', mediaError); release(); } };
}
