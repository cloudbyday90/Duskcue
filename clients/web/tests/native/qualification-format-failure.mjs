// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { formatSeeks } from './qualification-format-policy.mjs';

export async function formatFailureSnapshot(page, observation, stage) {
    const media = await page.evaluate(() => {
        const region = document.querySelector('[role="region"][aria-label="Media player"]');
        const video = region?.querySelector('video');
        const visible = (name) => [...region?.querySelectorAll('button') || []].some((button) => {
            const box = button.getBoundingClientRect();
            return button.getAttribute('aria-label') === name && box.width > 0 && box.height > 0 && !button.disabled;
        });
        return { ready: video?.readyState ?? null, paused: video?.paused ?? null, ended: video?.ended ?? null,
            currentTime: Number.isFinite(video?.currentTime) ? video.currentTime : null,
            duration: Number.isFinite(video?.duration) ? video.duration : null,
            width: video?.videoWidth ?? null, height: video?.videoHeight ?? null,
            playVisible: visible('Play'), pauseVisible: visible('Pause'), closeVisible: visible('Close player') };
    }).catch(() => ({ unavailable: true }));
    const seeks = formatSeeks(observation.records);
    const latest = seeks.at(-1);
    return { stage, media, successfulSeeks: seeks.length, latestSeek: latest ? { sessionId: latest.reply?.session_id,
        transcodeId: latest.reply?.transcode_session_id, positionMs: latest.reply?.position_ms } : null };
}
