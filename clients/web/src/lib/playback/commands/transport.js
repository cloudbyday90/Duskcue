// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { request, getServerOrigin } from '../../api/core.js';
import { scope, sameScope, valid } from './schema.js';

export async function commandSession() { return (await import('../../stores/auth.js')).auth; }

export function createCommandTransport({ session = null, send = request, origin = () => getServerOrigin() || globalThis.location?.origin } = {}) {
    function current(expected, cleanup = false) {
        const capture = session?.capture();
        if (!capture) return false;
        return capture.origin === expected.origin && capture.userId === expected.user && capture.context
            && sameScope(scope(origin(), capture.context), expected, cleanup);
    }
    async function context() {
        session ||= await commandSession();
        const began = performance.now();
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), 5000);
        try {
            const result = await session.readContext(controller.signal);
            controller.signal.throwIfAborted();
            valid(session.isCurrent(result.lease, true));
            return { context: result.context, scope: scope(origin(), result.context), began };
        } finally { clearTimeout(timer); }
    }
    async function call(method, path, expected, { body = undefined, etag = null, cleanup = false, timeoutMs = 5000 } = {}) {
        const began = performance.now();
        valid(current(expected, cleanup) && typeof path === 'string' && /^\/api\/v1\/playback-commands(?:\/[0-9a-f-]{36}(?:\/[a-z0-9/-]+)?)?$/.test(path));
        valid(Number.isSafeInteger(timeoutMs) && timeoutMs >= 1 && timeoutMs <= 5000);
        if (body !== undefined) valid(new TextEncoder().encode(JSON.stringify(body)).byteLength <= (path.endsWith('/checkpoint') || path.endsWith('/withdraw') ? 1024 : 32768));
        const controller = new AbortController();
        const timer = setTimeout(() => controller.abort(), timeoutMs);
        try {
            const result = await send(method, path, { root: true, body, profileScoped: !cleanup, signal: controller.signal,
                cache: 'no-store', redirect: 'error', maxJsonBytes: 32768, resultMetadata: true,
                headers: { ...(cleanup ? {} : { 'X-Session-Context': expected.context }), ...(etag ? { 'If-Match': etag } : {}) } });
            controller.signal.throwIfAborted();
            valid(current(expected, cleanup));
            return { ...result, began };
        } catch (error) {
            if (!current(expected, cleanup)) throw new Error('VOD_SCOPE_CHANGED');
            throw error;
        } finally { clearTimeout(timer); }
    }
    return { context, current, call };
}
