// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const requestFields = ['session_id', 'media_item_id', 'media_file_id', 'position_ms', 'force_hls', 'force_transcode', 'quality_mode',
    'max_streaming_bitrate', 'audio_stream_index', 'subtitle_stream_index', 'device_profile', 'direct_recovery_protocol', 'capability_protocol', 'source_capabilities'];
const replyFields = ['session_id', 'transcode_session_id', 'direct_source_revision', 'stream_decision', 'position_ms', 'stream_offset_ms',
    'source_route', 'copy_fallback', 'recovery', 'selected_audio_stream_index', 'selected_video_stream_index', 'target_video_codec', 'target_audio_codec'];
const select = (value, fields) => Object.fromEntries(fields.filter((key) => value?.[key] !== undefined).map((key) => [key, value[key]]));

export function observeSourceRequests(page, { origin, token }) {
    const records = [];
    const pending = new Set();
    let failed = false;
    let rateLimited = false;
    const observe = (response) => {
        const request = response.request();
        const url = new URL(request.url());
        if (!url.pathname.startsWith('/api/v1/')) return;
        const task = (async () => {
            if (url.origin !== origin || records.length >= 200 || url.search.length > 2048) throw new Error('Source requests exceeded their owned scope.');
            const headers = await request.allHeaders();
            const record = { method: request.method(), path: url.pathname, status: response.status(),
                bearer: headers.authorization === `Bearer ${token}`, cookie: Boolean(headers.cookie) };
            if (record.status === 429) {
                const retryAfter = await response.headerValue('retry-after');
                record.retryAfterSeconds = /^\d{1,4}$/.test(retryAfter || '') ? Number(retryAfter) : null;
                rateLimited = true;
            }
            if (record.method === 'GET' && /\/items\/[^/]+\/watch-data$/.test(record.path) && record.status === 200) {
                const watch = await response.json();
                record.watch = select(watch, ['resume_position_ms', 'is_watched']);
            }
            if (/^\/api\/v1\/playback\/(prepare|start|seek|stop)$/.test(record.path) && record.method === 'POST') {
                const body = request.postDataJSON();
                record.request = select(body, requestFields);
                const reply = await response.json();
                if (record.status === 200) record.reply = record.path.endsWith('/prepare') ? select(reply, ['protocol', 'source_revision', 'candidates']) : select(reply, replyFields);
                else record.error = { code: typeof reply.title === 'string' ? reply.title.slice(0, 80) : null,
                    detail: typeof reply.detail === 'string' ? reply.detail.slice(0, 400) : null };
                if (JSON.stringify(record).length > 32768) throw new Error('Source protocol evidence exceeded its bound.');
            }
            if (record.watch || ['prepare', 'start'].some((operation) => record.path === `/api/v1/playback/${operation}`) && record.method === 'POST') {
                if (await response.finished()) throw new Error('Source protocol response did not complete.');
                const timing = request.timing();
                if (!Number.isFinite(timing.startTime) || timing.startTime <= 0 || !Number.isFinite(timing.responseEnd) || timing.responseEnd < 0) throw new Error('Source request timing is unavailable.');
                record.timing = { startedAtMs: timing.startTime, completedAtMs: timing.startTime + timing.responseEnd };
            }
            records.push(record);
        })().catch(() => { failed = true; });
        pending.add(task);
        task.finally(() => pending.delete(task));
    };
    page.on('response', observe);
    return {
        records,
        async drain() {
            let timer;
            try {
                await Promise.race([Promise.all([...pending]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Source request observations did not drain.')), 3000); })]);
                if (rateLimited) throw new Error('A source request was rate limited by the production router.');
                if (failed) throw new Error('Source request observation failed.');
            } finally { clearTimeout(timer); }
        },
        detach() { page.off('response', observe); },
    };
}
