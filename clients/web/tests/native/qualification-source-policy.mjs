// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { sourcePreparation, sourceRoute } from '../../src/lib/playback/source-capabilities.js';
import { decodingAdmitted } from '../../src/lib/playback/decoding-capabilities.js';

const uuid = (value) => typeof value === 'string' && /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(value);
const failure = () => new Error('Actual source capability routing lost its canonical preparation, observation or ownership.');
const same = (left, right) => JSON.stringify(left) === JSON.stringify(right);
export const sourceSeeks = (records) => records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/seek' && record.status === 200);

export function assertSourceAuthentication(record, client) {
    if (record?.bearer !== (client === 'native') || record?.cookie !== (client === 'browser')) throw failure();
}

export function assertSourceStart(records, candidate, client) {
    const prepares = records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/prepare');
    const starts = records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/start');
    if (prepares.length !== 1 || starts.length !== 1 || prepares[0].status !== 200) throw failure();
    const prepare = prepares[0];
    const start = starts[0];
    for (const record of [prepare, start]) {
        assertSourceAuthentication(record, client);
        if (record.request?.media_item_id !== candidate.item || record.request.media_file_id !== candidate.file
            || record.request.quality_mode !== 'auto' || record.request.max_streaming_bitrate !== null
            || record.request.device_profile !== null || record.request.force_transcode !== false
            || record.request.audio_stream_index !== null || record.request.subtitle_stream_index !== null
            || Boolean(record.request.force_hls) !== (client === 'native')) throw failure();
    }
    if (prepare.request.capability_protocol !== 1) throw failure();
    for (const key of ['media_item_id', 'media_file_id', 'quality_mode', 'max_streaming_bitrate', 'device_profile', 'force_transcode',
        'force_hls', 'audio_stream_index', 'subtitle_stream_index']) if (!same(prepare.request[key], start.request[key])) throw failure();
    const preparation = sourcePreparation(prepare.reply, { requiresHeaders: client === 'native', forceHls: client === 'native' });
    const observations = start.request.source_capabilities;
    if (!preparation || observations?.version !== 1 || observations.source_revision !== preparation.source_revision
        || !Array.isArray(observations.observations) || observations.observations.length !== preparation.candidates.length) throw failure();
    for (const [index, value] of observations.observations.entries()) {
        if (value.candidate_id !== preparation.candidates[index].id || !['reported', 'unavailable', 'rejected', 'timed_out'].includes(value.status)) throw failure();
        for (const key of ['supported', 'smooth', 'power_efficient', 'delivery_supported']) if (value[key] !== null && typeof value[key] !== 'boolean') throw failure();
    }
    const eligible = observations.observations.find(decodingAdmitted);
    if (candidate.dynamicRange !== 'sdr') {
        if (preparation.candidates.length || start.status !== 422 || start.error?.code !== 'PLAY_020') throw failure();
        return { prepare, start, preparation, observations, route: null, native: false, optimized: false };
    }
    if (start.status !== 200 || !uuid(start.reply?.session_id)) throw failure();
    const route = sourceRoute(start.reply.source_route);
    if (eligible ? !route || route.candidate_id !== eligible.candidate_id || route.source_revision !== preparation.source_revision : start.reply.source_route != null) throw failure();
    const native = route?.candidate_id === 'native_file';
    if (native ? client !== 'browser' || start.reply.stream_decision !== 'direct_play' || start.reply.transcode_session_id != null
        || !uuid(start.reply.direct_source_revision) : !['transcode', 'direct_stream'].includes(start.reply.stream_decision) || !uuid(start.reply.transcode_session_id)) throw failure();
    return { prepare, start, preparation, observations, route, native, optimized: Boolean(route) };
}

export function sourceOwnership(reply, native) {
    const value = native ? reply?.direct_source_revision : reply?.transcode_session_id;
    if (!uuid(value) || native && reply?.transcode_session_id != null) throw failure();
    return value;
}

export function assertSourceSeek(record, { sessionId, previousOwner, positionMs, client, native }) {
    assertSourceAuthentication(record, client);
    if (record?.method !== 'POST' || record.path !== '/api/v1/playback/seek' || record.status !== 200
        || record.request?.session_id !== sessionId || record.request.position_ms !== positionMs
        || record.reply?.session_id !== sessionId || record.reply.position_ms !== positionMs) throw failure();
    const owner = sourceOwnership(record.reply, native);
    if (owner === previousOwner) throw failure();
    return owner;
}

export function initialSourceOwnership(records, contract, candidate, client) {
    const watch = records.find((record) => record.method === 'GET' && record.path === `/api/v1/items/${candidate.item}/watch-data` && record.status === 200);
    assertSourceAuthentication(watch, client);
    const raw = Number(watch?.watch?.resume_position_ms);
    const positionMs = watch?.watch?.is_watched || !Number.isFinite(raw) ? 0 : Math.max(0, raw);
    if (!Number.isSafeInteger(positionMs) || positionMs >= 8000) throw failure();
    const seeks = sourceSeeks(records);
    const automatic = !contract.native && positionMs > 0;
    if (seeks.length !== (automatic ? 1 : 0)) throw failure();
    const startOwner = sourceOwnership(contract.start.reply, contract.native);
    const owner = automatic ? assertSourceSeek(seeks[0], { sessionId: contract.start.reply.session_id, previousOwner: startOwner, positionMs, client, native: false }) : startOwner;
    return { positionMs, owner, automaticSeek: seeks[0] || null, seekCount: seeks.length };
}

export function assertSourceMedia(records, candidate, owner, client, native) {
    const media = records.filter((record) => record.method === 'GET' && (native ? record.path === `/api/v1/stream/${candidate.file}` : record.path.includes(`/transcode/${owner}/`)));
    if (native) {
        if (!media.some((record) => [200, 206].includes(record.status)) || media.some((record) => ![200, 206, 304].includes(record.status))) throw failure();
    } else for (const extension of ['.m3u8', '.mp4', '.m4s']) if (!media.some((record) => record.path.endsWith(extension) && record.status === 200)) throw failure();
    for (const record of media) assertSourceAuthentication(record, client);
}

export function assertSourcePresentation(value, { owner, native, width, height, targetPositionMs = null, streamOffsetMs = 0 }) {
    if (value?.owner !== owner || value.native !== native || value.currentSource !== value.source
        || (native ? !/^https?:\/\//.test(value.source || '') : !value.source?.startsWith('blob:'))
        || value.ready < 2 || value.width !== width || value.height !== height || !Number.isSafeInteger(value.frames) || value.frames < 5
        || !Number.isFinite(value.firstMediaTime) || !Number.isFinite(value.lastMediaTime) || value.lastMediaTime <= value.firstMediaTime + 0.4) throw failure();
    if (targetPositionMs !== null && (!Number.isSafeInteger(streamOffsetMs) || streamOffsetMs < 0
        || value.firstMediaTime * 1000 + streamOffsetMs < targetPositionMs - 2
        || value.firstMediaTime * 1000 + streamOffsetMs > targetPositionMs + 250)) throw new Error('The first presented source frame differs from the canonical seek target.');
    return value;
}
