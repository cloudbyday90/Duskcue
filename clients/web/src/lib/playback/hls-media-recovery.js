// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function createHlsMediaRecovery({ video, isCurrent, retry, resume, onFailure }) {
    let first = null;
    let reloaded = false;
    let finished = false;
    let timer = null;
    const position = () => Number.isFinite(video.currentTime) && video.currentTime >= 0 ? video.currentTime : 0;

    function clearDeadline() {
        clearTimeout(timer);
        timer = null;
    }

    function dispose() {
        finished = true;
        clearDeadline();
        video.removeEventListener?.('loadedmetadata', loaded);
        video.removeEventListener?.('timeupdate', progress);
    }

    function fail(error, seconds) {
        if (finished || !isCurrent()) return;
        dispose();
        onFailure(error, seconds);
    }

    function loaded() {
        if (finished || !first || !isCurrent()) return;
        reloaded = true;
        if (!video.error) resume();
    }

    function progress() {
        if (!finished && first && isCurrent() && !video.error && video.readyState >= 2 && position() > first.position + 0.05) clearDeadline();
    }

    video.addEventListener?.('loadedmetadata', loaded);
    video.addEventListener?.('timeupdate', progress);
    return {
        report(error, origin) {
            if (finished || !isCurrent()) return;
            if (first) {
                if (!reloaded && timer !== null && origin !== first.origin) return;
                fail(error, Math.max(first.position, position()));
                return;
            }
            first = { error, origin, position: position() };
            timer = setTimeout(() => fail(first.error, Math.max(first.position, position())), 5000);
            try { retry(); }
            catch { fail(first.error, first.position); }
        },
        dispose,
    };
}
