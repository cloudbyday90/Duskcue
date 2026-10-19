// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { sourcePreparation, sourceRoute } from '../../src/lib/playback/source-capabilities.js';
import { decodingAdmitted } from '../../src/lib/playback/decoding-capabilities.js';
import { assertSourceAuthentication } from './qualification-source-policy.mjs';

const uuid = (value) => typeof value === 'string' && /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i.test(value);
const invalid = () => new Error('Actual copied-source decoder recovery evidence is incomplete or lost its ownership.');
const validTiming = (record) => Number.isFinite(record.timing?.startedAtMs) && record.timing.startedAtMs > 0
    && Number.isFinite(record.timing.completedAtMs) && record.timing.completedAtMs >= record.timing.startedAtMs;

export function sourceRecoveryCandidate(ready, client) {
    const variant = client === 'browser' ? 'video-slice' : client === 'native' ? 'video-slice-native' : null;
    const candidates = ready.sourceRecoveryCandidates;
    const selected = variant && Array.isArray(candidates) && candidates.length <= 4 ? candidates.filter((candidate) => candidate.variant === variant) : [];
    const value = selected.length === 1 ? selected[0] : null;
    if (!value || !uuid(value.item) || !uuid(value.file) || !/^[a-f0-9]{64}$/.test(value.sha256 || '')
        || value.videoIndex !== 1 || value.audioIndex !== 2 || value.audioHz !== 880) throw invalid();
    if (candidates.some((other) => other !== value && ['video-slice', 'video-slice-native'].includes(other.variant)
        && (other.item === value.item || other.file === value.file))) throw invalid();
    return value;
}

export function sourceRecoveryContract(records, candidate, client) {
    const variant = client === 'browser' ? 'video-slice' : client === 'native' ? 'video-slice-native' : null;
    if (!variant || candidate.variant !== variant) throw invalid();
    const watches = records.filter((record) => record.method === 'GET' && record.path === `/api/v1/items/${candidate.item}/watch-data`);
    const preparations = records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/prepare');
    const starts = records.filter((record) => record.method === 'POST' && record.path === '/api/v1/playback/start');
    if (watches.length !== 1 || watches[0].status !== 200 || watches[0].watch?.resume_position_ms !== 0 || watches[0].watch.is_watched !== false
        || preparations.length !== 1 || starts.length !== 1 || preparations[0].status !== 200 || starts[0].status !== 200) throw invalid();
    const watch = watches[0];
    const prepare = preparations[0];
    const start = starts[0];
    assertSourceAuthentication(watch, client);
    if (![watch, prepare, start].every(validTiming) || watch.timing.completedAtMs > prepare.timing.startedAtMs
        || watch.timing.completedAtMs > start.timing.startedAtMs) throw invalid();
    for (const record of [prepare, start]) {
        assertSourceAuthentication(record, client);
        if (record.request?.media_item_id !== candidate.item || record.request.media_file_id !== candidate.file
            || record.request.quality_mode !== 'auto' || record.request.max_streaming_bitrate !== null || record.request.device_profile !== null
            || record.request.force_hls !== true || record.request.force_transcode !== false || record.request.subtitle_stream_index !== null
            || record.request.audio_stream_index !== (client === 'browser' ? 2 : null)) throw invalid();
    }
    for (const key of ['media_item_id', 'media_file_id', 'quality_mode', 'max_streaming_bitrate', 'force_hls', 'force_transcode', 'audio_stream_index', 'subtitle_stream_index']) {
        if (prepare.request[key] !== start.request[key]) throw invalid();
    }
    const preparation = sourcePreparation(prepare.reply, { forceHls: true, requiresHeaders: client === 'native' });
    const observations = start.request.source_capabilities;
    const route = sourceRoute(start.reply?.source_route);
    if (prepare.request.capability_protocol !== 1 || !preparation || observations?.version !== 1 || observations.source_revision !== preparation.source_revision
        || !Array.isArray(observations.observations) || observations.observations.length !== preparation.candidates.length
        || !route || route.source_revision !== preparation.source_revision || !route.candidate_id.startsWith('hls_video_copy_')
        || !uuid(start.reply.session_id) || !uuid(start.reply.transcode_session_id) || !['direct_stream', 'transcode'].includes(start.reply.stream_decision)) throw invalid();
    for (const [index, observation] of observations.observations.entries()) if (observation.candidate_id !== preparation.candidates[index].id) throw invalid();
    if (observations.observations.find(decodingAdmitted)?.candidate_id !== route.candidate_id) throw invalid();
    const fallback = start.reply.copy_fallback;
    if (fallback?.generation !== start.reply.transcode_session_id) throw invalid();
    return { watch, prepare, start, preparation, observations, route };
}

export function assertSourceDecoderRecovery(records, sessionId, generation, errors, client) {
    const replies = records.filter((record) => record.path === '/api/v1/playback/heartbeat' && record.status === 200
        && record.request?.session_id === sessionId && record.request.transcode_session_id === generation && record.request.recovery_requested === true);
    if (replies.length !== 1) throw invalid();
    const heartbeat = replies[0];
    assertSourceAuthentication(heartbeat, client);
    const ack = heartbeat.request.copy_fallback;
    const recovery = heartbeat.reply?.recovery;
    if (ack?.generation !== generation || ack.supported !== true || ack.smooth !== true
        || heartbeat.reply.session_id !== sessionId || !uuid(heartbeat.reply.transcode_session_id) || heartbeat.reply.transcode_session_id === generation
        || heartbeat.reply.stream_decision !== 'transcode' || heartbeat.reply.copy_fallback != null
        || !Number.isSafeInteger(heartbeat.reply.stream_offset_ms) || heartbeat.reply.stream_offset_ms < 0
        || heartbeat.reply.position_ms !== heartbeat.request.position_ms || recovery?.status !== 'recovered'
        || recovery.reason !== 'client_decode' || recovery.attempts !== 1 || recovery.skipped_ms !== 0 || typeof recovery.quality_reduced !== 'boolean') throw invalid();
    const decoderError = errors.find((error) => error.code === 3 && error.trusted === true && error.generation === generation && error.source?.startsWith('blob:'));
    if (!decoderError) throw invalid();
    return { heartbeat, decoderError, generation: heartbeat.reply.transcode_session_id, recovery,
        replacementOffsetMs: heartbeat.reply.stream_offset_ms, recoveryPositionMs: heartbeat.reply.position_ms };
}
