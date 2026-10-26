/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { request } from '../api/core.js';
import { exactReviver, etag, id, requireValue } from './schema.js';

const problemTitles = ['AUTH_005', 'VALID_001', ...Array.from({ length: 7 }, (_, i) => `OWNER_00${i + 1}`), ...Array.from({ length: 7 }, (_, i) => `IPTV_00${i + 1}`), ...Array.from({ length: 6 }, (_, i) => `IPTV_REFRESH_00${i + 1}`)];
export const assignmentPath = (household) => `/live-tv/households/${id(household)}/assignment`;
export const refreshPath = (household) => `/live-tv/households/${id(household)}/refresh`;
export const ownerPath = '/household-management/live-tv';
export const controlPath = '/live-tv/manual-refresh-control';

export async function managementRequest(method, path, { context = null, body = undefined, version = null, create = false, params = {}, signal = null } = {}) {
    requireValue(context && /^sc1\.[A-Za-z0-9_-]{43}$/.test(context.context_id));
    if (body !== undefined) requireValue(new TextEncoder().encode(JSON.stringify(body)).byteLength <= 16384);
    const headers = { 'X-Session-Context': context.context_id };
    if (version) headers['If-Match'] = etag(version);
    if (create) headers['If-None-Match'] = '*';
    const controller = new AbortController(); const cancel = () => controller.abort();
    signal?.addEventListener('abort', cancel, { once: true }); if (signal?.aborted) cancel();
    const timer = setTimeout(cancel, 15000);
    try {
        return await request(method, path, { body, params, headers, signal: controller.signal, profileScoped: method === 'GET', cache: 'no-store', redirect: 'error', maxJsonBytes: 262144, jsonReviver: exactReviver, problemTitles, resultMetadata: true });
    } finally { clearTimeout(timer); signal?.removeEventListener('abort', cancel); }
}

export async function representation(path, context, project, signal = null, params = {}) {
    const response = await managementRequest('GET', path, { context, signal, params });
    requireValue(response.status === 200);
    return { body: project(response.body), etag: etag(response.etag) };
}
