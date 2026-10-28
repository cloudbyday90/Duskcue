/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { request } from './core.js';
import { sessionContext } from '../household/schema.js';

const problemTitles = ['AUTH_005', ...Array.from({ length: 7 }, (_, index) => `OWNER_00${index + 1}`)];
export async function householdRequest(method, path, { body = undefined, context = null, signal = null, profileScoped = false, maxJsonBytes = 262_144 } = {}) {
    const controller = new AbortController();
    const cancel = () => controller.abort();
    signal?.addEventListener('abort', cancel, { once: true });
    if (signal?.aborted) cancel();
    const timer = setTimeout(cancel, 15_000);
    try {
        if (body !== undefined && new TextEncoder().encode(JSON.stringify(body)).byteLength > 65_536) throw new Error('Management request exceeds its budget');
        return await request(method, path, { body, signal: controller.signal, profileScoped, cache: 'no-store', maxJsonBytes, problemTitles,
            headers: context ? { 'X-Session-Context': context.context_id } : {} });
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', cancel); }
}

export async function getSessionContext(signal = null) {
    return sessionContext(await householdRequest('GET', '/auth/session-context', { signal, profileScoped: true, maxJsonBytes: 32768 }));
}

export function getOwnerStatus(context, signal = null) { return householdRequest('GET', '/household-management/status', { context, signal }); }
export function startOwnerPasskey(body, context, signal = null) { return householdRequest('POST', '/household-management/passkey/start', { body, context, signal }); }
export function listOwnerCandidates(signal = null) { return householdRequest('GET', '/user/passkeys', { signal }); }
