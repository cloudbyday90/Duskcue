// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const uuid = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
const positive = (value) => Number.isSafeInteger(value) && value > 0;

export function assertNativeRecoveryArm(ack, expected) {
    if (!uuid.test(expected?.resourceId || '') || !uuid.test(expected?.sessionId || '') || !uuid.test(expected?.transcodeId || '')
        || expected.positionMs !== 4000 || expected.paused !== true
        || ack?.resourceId !== expected.resourceId || ack.sessionId !== expected.sessionId || ack.transcodeId !== expected.transcodeId
        || ack.positionMs !== 4000 || ack.paused !== true || ack.armed !== true
        || !positive(ack.encoderIdentity?.pid) || !positive(ack.encoderIdentity?.parent_pid) || !positive(ack.encoderIdentity?.started_ticks)) {
        throw new Error('Native recovery arm did not identify the exact owned paused worker.');
    }
    return { resourceId: ack.resourceId, sessionId: ack.sessionId, transcodeId: ack.transcodeId, positionMs: 4000, paused: true, armed: true,
        encoderIdentity: { pid: ack.encoderIdentity.pid, parent_pid: ack.encoderIdentity.parent_pid, started_ticks: ack.encoderIdentity.started_ticks } };
}

export function assertNaturalNativeRecovery(record, expected) {
    const request = record?.request;
    const reply = record?.reply;
    const recovery = reply?.recovery;
    if (!uuid.test(expected?.sessionId || '') || !uuid.test(expected?.transcodeId || '') || expected.positionMs !== 4000 || expected.paused !== true
        || record?.method !== 'POST' || record.path !== '/api/v1/playback/heartbeat' || record.status !== 200 || record.bearer !== true || record.cookie !== false
        || request?.session_id !== expected.sessionId || request.transcode_session_id !== expected.transcodeId
        || request.position_ms !== 4000 || request.state !== 'paused' || typeof request.is_buffering !== 'boolean'
        || request.recovery_requested !== undefined && request.recovery_requested !== false
        || reply?.session_id !== expected.sessionId || !uuid.test(reply.transcode_session_id || '') || reply.transcode_session_id === expected.transcodeId
        || !['transcode', 'direct_stream'].includes(reply.stream_decision) || reply.position_ms !== 4000 || reply.stream_offset_ms !== 4000
        || recovery?.status !== 'recovered' || recovery.reason !== 'stalled' || recovery.attempts !== 1 || recovery.skipped_ms !== 0 || recovery.quality_reduced !== false) {
        throw new Error('A natural paused heartbeat did not establish one owned stalled-worker replacement.');
    }
    return record;
}
