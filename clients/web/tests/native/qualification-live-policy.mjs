// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
const userFields = ['id', 'username', 'display_name', 'role', 'capabilities', 'has_all_library_access', 'active_profile_id', 'profile_selection_required', 'locale'];

export function nativeLiveFailure(error, token, stage) {
    if (stage === 'isolation') return 'Native isolation check failed.';
    return String(error?.message || 'Native operation failed.').replaceAll(token, '[redacted]')
        .replace(/Bearer\s+[A-Za-z0-9_.-]+/gi, 'Bearer [redacted]').slice(0, 1200);
}

export function nativeLiveScenario(manifest, ready, gatewayOrigin) {
    const origin = new URL(gatewayOrigin);
    if (origin.origin !== gatewayOrigin || origin.origin !== manifest.origins?.[0]
        || origin.protocol !== 'http:' || !/^127\.\d+\.\d+\.\d+$/.test(origin.hostname) || origin.port !== '48027'
        || origin.username || origin.password) throw new Error('Native live gateway must match the isolated public-port origin.');
    const candidate = ready.nativeCandidate || ready.cleanCandidate || ready.candidate || ready.candidates?.[0];
    const user = ready.user;
    if (!uuid.test(ready.resourceId || '') || !/^[A-Za-z0-9_-]{32,256}$/.test(ready.token || '')
        || !uuid.test(user?.id || '') || !uuid.test(user?.active_profile_id || '')
        || user.profile_selection_required !== false || typeof user.username !== 'string'
        || !uuid.test(candidate?.item || '') || !uuid.test(candidate?.file || '')) throw new Error('Native live fixture identity is incomplete.');
    const cachedUser = Object.fromEntries(userFields.filter((field) => user[field] !== undefined).map((field) => [field, user[field]]));
    if (JSON.stringify(cachedUser).length > 16384 || JSON.stringify(cachedUser).includes(ready.token)) throw new Error('Native live user cache is invalid.');
    return { origin: origin.origin, token: ready.token, user: cachedUser, candidate,
        userStorageKey: `duskcue_user:${encodeURIComponent(origin.origin)}` };
}

export function assertNativeLiveMedia(records) {
    const media = records.filter((record) => record.method === 'GET' && /\/transcode\/[^/]+\//.test(record.path));
    if (!media.some((record) => record.path.endsWith('.m3u8') && record.status === 200)
        || !media.some((record) => record.path.endsWith('.m4s') && record.status === 200)
        || media.some((record) => !record.bearer || record.cookie)) throw new Error('Actual bearer-authenticated native HLS was not established.');
    const starts = records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/start' && record.status === 200);
    if (starts.length !== 1 || !starts[0].bearer || starts[0].cookie || starts[0].request?.force_hls !== true || starts[0].reply?.stream_decision === 'direct_play'
        || !uuid.test(starts[0].reply?.session_id || '') || !uuid.test(starts[0].reply?.transcode_session_id || '')) throw new Error('Native playback did not retain its authenticated HLS start contract.');
    return { starts, media };
}
