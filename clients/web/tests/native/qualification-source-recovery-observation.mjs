// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { observeSourceRequests } from './qualification-source-observation.mjs';

export function installSourceRecoveryErrors() {
    if (window.sourceRecoveryErrors) throw new Error('Source decoder error observation is already active.');
    const errors = [];
    let generation = null;
    const failed = (event) => {
        if (!(event.target instanceof HTMLVideoElement) || errors.length >= 8) return;
        errors.push({ code: event.target.error?.code ?? null, trusted: event.isTrusted, generation,
            source: event.target.currentSrc, mediaTime: Number.isFinite(event.target.currentTime) ? event.target.currentTime : null,
            observedAt: performance.now() });
    };
    document.addEventListener('error', failed, true);
    window.sourceRecoveryErrors = {
        arm(value) { if (generation || errors.length) throw new Error('The source decoder failed before its healthy-copy checkpoint.'); generation = value; },
        snapshot() { return errors.map((value) => ({ ...value })); },
        detach() { document.removeEventListener('error', failed, true); delete window.sourceRecoveryErrors; },
    };
}

export function observeSourceRecoveryRequests(page, context) {
    const base = observeSourceRequests(page, context);
    const heartbeats = [];
    const pending = new Set();
    let failed = false;
    const observe = (response) => {
        const request = response.request();
        const url = new URL(request.url());
        if (url.pathname !== '/api/v1/playback/heartbeat' || request.method() !== 'POST') return;
        const task = (async () => {
            if (url.origin !== context.origin || heartbeats.length >= 32 || response.status() === 429) throw new Error('Source recovery heartbeat scope failed.');
            const headers = await request.allHeaders();
            const body = request.postDataJSON();
            const reply = response.status() === 200 ? await response.json() : null;
            const select = (value, fields) => Object.fromEntries(fields.filter((key) => value?.[key] !== undefined).map((key) => [key, value[key]]));
            const record = { method: 'POST', path: url.pathname, status: response.status(), bearer: headers.authorization === `Bearer ${context.token}`,
                cookie: Boolean(headers.cookie), request: select(body, ['session_id', 'position_ms', 'state', 'is_buffering', 'is_paused', 'transcode_session_id', 'recovery_requested', 'copy_fallback']),
                reply: select(reply, ['session_id', 'position_ms', 'stream_decision', 'transcode_session_id', 'stream_offset_ms', 'copy_fallback', 'recovery']) };
            if (JSON.stringify(record).length > 8192) throw new Error('Source recovery heartbeat exceeded its evidence bound.');
            heartbeats.push(record);
        })().catch(() => { failed = true; });
        pending.add(task);
        task.finally(() => pending.delete(task));
    };
    page.on('response', observe);
    return {
        get records() { return [...base.records, ...heartbeats]; },
        async drain() {
            await base.drain();
            let timer;
            try {
                await Promise.race([Promise.all([...pending]), new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error('Source recovery observations did not drain.')), 3000); })]);
                if (failed) throw new Error('Source recovery heartbeat observation failed.');
            } finally { clearTimeout(timer); }
        },
        detach() { base.detach(); page.off('response', observe); },
    };
}
