// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { assertNativeLiveMedia } from '../../clients/web/tests/native/qualification-live-policy.mjs';
import { assertNativeRecoveryArm, assertNaturalNativeRecovery } from '../../clients/web/tests/native/qualification-live-recovery-policy.mjs';

const uuid = /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i;
const media = (decoded, audio) => Number.isSafeInteger(decoded?.frames) && decoded.frames >= 5
    && decoded.currentTime >= 0.4 && decoded.width > 0 && decoded.height > 0 && audio?.running === true
    && Number.isFinite(audio.frequency) && Math.abs(audio.frequency - 880) < 20
    && Number.isFinite(audio.level) && audio.level > -60 && Number.isFinite(audio.elapsedMs) && audio.elapsedMs >= 500;

export function validateNativeRecoveryProof(native, checkpoint, resourceId) {
    const recovery = native?.checks?.recovery;
    const expected = { resourceId, sessionId: recovery?.sessionId, transcodeId: recovery?.previousTranscodeId, positionMs: 4000, paused: true };
    const armed = assertNativeRecoveryArm(recovery?.armed, expected);
    assertNaturalNativeRecovery(recovery.heartbeat, expected);
    const { starts } = assertNativeLiveMedia(native.requests || []);
    const requests = native.requests;
    const heartbeatObserved = requests.some((record) => {
        try { assertNaturalNativeRecovery(record, expected); return record.reply.transcode_session_id === recovery.transcodeId; }
        catch { return false; }
    });
    const seekObserved = (position, generation) => requests.some((record) => record.method === 'POST' && record.path === '/api/v1/playback/seek'
        && record.status === 200 && record.bearer === true && record.cookie === false && record.request?.session_id === expected.sessionId
        && record.request.position_ms === position && record.reply?.session_id === expected.sessionId && record.reply.position_ms === position
        && record.reply.transcode_session_id === generation);
    const proof = checkpoint?.native_recovery;
    const seek = native.checks.seek;
    const generationMedia = (generation) => ['.m3u8', '.m4s'].every((extension) => requests.some((record) => record.method === 'GET'
        && record.path.includes(`/transcode/${generation}/`) && record.path.endsWith(extension) && record.status === 200 && record.bearer === true && record.cookie === false));
    if (!heartbeatObserved || !seekObserved(4000, expected.transcodeId) || !seekObserved(6000, seek?.transcodeId)
        || !generationMedia(recovery.transcodeId) || !generationMedia(seek?.transcodeId)
        || starts[0].reply.session_id !== expected.sessionId || recovery.transcodeId !== recovery.heartbeat.reply.transcode_session_id
        || recovery.positionMs !== 4000 || recovery.attempts !== 1 || recovery.preservedPause !== true || recovery.unexpectedPlayEvents !== 0
        || recovery.notice !== 'A more compatible playback stream is ready.' || recovery.freshBlobSource !== true || recovery.backend?.passed !== true
        || !media(recovery.decoded, recovery.audio) || seek?.positionMs !== 6000 || seek.preservedPause !== true || seek.freshBlobSource !== true
        || !uuid.test(seek.transcodeId || '') || [expected.transcodeId, recovery.transcodeId].includes(seek.transcodeId) || !media(seek.decoded, seek.audio)
        || proof?.reason !== 'stalled' || proof.attempts !== 1 || proof.skipped_ms !== 0 || proof.quality_reduced !== false || proof.paused !== true
        || proof.frozen_encoder_gone !== true || proof.stalled_cache_retired !== true || proof.stalled_generation !== expected.transcodeId
        || proof.recovered_generation !== recovery.transcodeId
        || ['pid', 'parent_pid', 'started_ticks'].some((key) => proof.encoder_identity?.[key] !== armed.encoderIdentity[key])) {
        throw new Error('Native stalled-worker recovery, fresh presentation or physical retirement proof is incomplete.');
    }
}
