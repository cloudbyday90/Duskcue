// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const requestFields = ['session_id', 'media_item_id', 'media_file_id', 'position_ms', 'force_hls', 'force_transcode', 'quality_mode', 'max_streaming_bitrate', 'audio_stream_index', 'subtitle_stream_index'];
const replyFields = ['session_id', 'transcode_session_id', 'stream_decision', 'position_ms', 'stream_offset_ms', 'recovery'];
const select = (value, fields) => Object.fromEntries(fields.filter((key) => value?.[key] !== undefined).map((key) => [key, value[key]]));

export function observeFormatRequests(page, { origin, token }) {
    const records = [];
    const pending = new Set();
    let failed = false;
    let rateLimited = false;
    const observe = (response) => {
        const request = response.request();
        const url = new URL(request.url());
        if (!url.pathname.startsWith('/api/v1/')) return;
        const task = (async () => {
            if (url.origin !== origin || records.length >= 200 || url.search.length > 2048) throw new Error('Format requests exceeded their owned scope.');
            const headers = await request.allHeaders();
            const record = { method: request.method(), path: url.pathname, status: response.status(), bearer: headers.authorization === `Bearer ${token}`, cookie: Boolean(headers.cookie) };
            if (record.status === 429) {
                const retryAfter = await response.headerValue('retry-after');
                record.retryAfterSeconds = /^\d{1,4}$/.test(retryAfter || '') ? Number(retryAfter) : null;
                rateLimited = true;
            }
            if (record.method === 'GET' && /\/items\/[^/]+\/watch-data$/.test(record.path) && record.status === 200) {
                const watch = await response.json();
                record.watch = { resume_position_ms: watch.resume_position_ms, is_watched: watch.is_watched };
            }
            if (/^\/api\/v1\/playback\/(start|seek|stop)$/.test(url.pathname) && request.method() === 'POST') {
                const body = request.postDataJSON();
                record.request = select(body, requestFields);
                if (url.pathname.endsWith('/start')) record.request.device_profile = body.device_profile === null ? null : 'non-null';
                const reply = await response.json();
                if (response.status() === 200) record.reply = select(reply, replyFields);
                else record.error = { code: typeof reply.title === 'string' ? reply.title.slice(0, 80) : null,
                    detail: typeof reply.detail === 'string' ? reply.detail.slice(0, 400) : null };
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
                await Promise.race([Promise.all([...pending]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Format request observations did not drain.')), 3000); })]);
                if (rateLimited) throw new Error('A format request was rate limited by the production router.');
                if (failed) throw new Error('Format request observation failed.');
            } finally { clearTimeout(timer); }
        },
        detach() { page.off('response', observe); },
    };
}
