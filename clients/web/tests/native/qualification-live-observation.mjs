// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const requestFields = ['session_id', 'media_item_id', 'media_file_id', 'position_ms', 'force_hls', 'force_transcode', 'audio_stream_index', 'subtitle_stream_index', 'transcode_session_id', 'state', 'is_buffering', 'recovery_requested'];
const replyFields = ['session_id', 'position_ms', 'stream_decision', 'transcode_session_id', 'stream_offset_ms', 'recovery'];
const select = (value, fields) => Object.fromEntries(fields.filter((field) => value?.[field] !== undefined).map((field) => [field, value[field]]));

export function observeNativeLive(page, scenario) {
    const records = [];
    const pending = new Set();
    let failed = false;
    const observe = (response) => {
        const request = response.request();
        const url = new URL(request.url());
        if (!url.pathname.startsWith('/api/v1/')) return;
        const task = (async () => {
            if (url.origin !== scenario.origin || records.length >= 1000 || url.search.length > 2048) throw new Error('Native API request exceeded its owned scope.');
            const headers = await request.allHeaders();
            const record = { method: request.method(), path: url.pathname, status: response.status(), bearer: headers.authorization === `Bearer ${scenario.token}`, cookie: Boolean(headers.cookie) };
            if (/^\/api\/v1\/playback\/(start|heartbeat|seek|stop)$/.test(url.pathname) && request.method() === 'POST') {
                record.request = select(request.postDataJSON(), requestFields);
                if (response.status() === 200) record.reply = select(await response.json(), replyFields);
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
                await Promise.race([Promise.all([...pending]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Native request observations did not drain.')), 3000); })]);
                if (failed) throw new Error('Native request observation failed.');
            } finally { clearTimeout(timer); }
        },
        detach() { page.off('response', observe); },
    };
}
