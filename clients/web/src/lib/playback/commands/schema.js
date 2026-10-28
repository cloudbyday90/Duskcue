// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { instant as parseInstant } from '../../time/instant.js';

export const PROTOCOL = 'duskcue-vod-command/2';
export const uuid = (value) => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
export const entityId = (value) => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
export function valid(condition) { if (!condition) throw new Error('VOD_RESPONSE_INVALID'); }
export function capability(value) {
    if (value === undefined) return null;
    const features = ['start-settlement', 'conditional-controls', 'checkpoints', 'lease-observation'];
    valid(value?.protocol === PROTOCOL && Array.isArray(value.features) && value.features.length === features.length
        && features.every((feature) => value.features.includes(feature)));
    return PROTOCOL;
}
export function scope(origin, context) {
    return { origin: new URL(origin).origin, user: context.user_id, session: context.session_id,
        credential: context.credential_revision, transport: context.transport, profile: context.profile_id,
        revision: context.profile_context_revision, context: context.context_id };
}
export function sameScope(a, b, cleanup = false) {
    const keys = ['origin', 'user', 'session', 'credential', 'transport', ...(cleanup ? [] : ['profile', 'revision', 'context'])];
    return !!a && !!b && keys.every((key) => a[key] === b[key]);
}
export function path(id, suffix = '') {
    valid(uuid(id));
    return `/api/v1/playback-commands/${id}${suffix}`;
}
export function resource(value, origin, expected) {
    valid(typeof value === 'string' && value.length <= 512 && value.startsWith('/api/v1/'));
    const url = new URL(value, origin);
    valid(url.origin === origin && !url.username && !url.password && !url.search && !url.hash && url.pathname === expected);
    return url.href;
}
export function instant(value) {
    return parseInstant(value).milliseconds;
}
export function receipt(value, id, origin) {
    valid(value?.protocol === PROTOCOL && value.request_id === id && ['unknown', 'preparing', 'ready', 'changing', 'unavailable', 'uncertain', 'stopping', 'released', 'failed', 'withdrawing', 'withdrawn'].includes(value.status));
    resource(value.status_url, origin, path(id));
    instant(value.observed_at);
    if (value.lease_deadline !== null) instant(value.lease_deadline);
    if (value.status === 'ready') {
        const ready = value.ready;
        valid(ready && uuid(ready.session_id) && uuid(ready.output_revision) && entityId(ready.media_item_id) && entityId(ready.media_file_id)
            && ['direct_play', 'direct_stream', 'transcode'].includes(ready.delivery_mode)
            && Number.isSafeInteger(ready.position_ms) && ready.position_ms >= 0
            && (ready.delivery_mode === 'direct_play' && ready.stream_offset_ms === null || Number.isSafeInteger(ready.stream_offset_ms) && ready.stream_offset_ms >= 0) && value.lease_deadline);
        const suffix = ready.delivery_mode === 'direct_play' ? '/file' : '/hls/manifest.m3u8';
        resource(value.stream_url, origin, path(id, `/outputs/${ready.output_revision}${suffix}`));
    } else valid(value.ready === null && value.stream_url === null);
    return value;
}
export function settlement(value, id, origin) {
    valid(value?.protocol === PROTOCOL && value.request_id === id && ['pending', 'uncertain', 'settled'].includes(value.status)
        && typeof value.admission_fenced === 'boolean' && typeof value.preparation_complete === 'boolean'
        && [null, true, false].includes(value.parent_released));
    resource(value.status_url, origin, path(id, '/settlement'));
    instant(value.observed_at);
    if (value.status === 'settled') valid(value.admission_fenced && value.preparation_complete && value.parent_released !== false);
    return value;
}
