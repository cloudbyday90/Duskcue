// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { assertNativeRecoveryArm, assertNaturalNativeRecovery } from './qualification-live-recovery-policy.mjs';

const resourceId = '00000000-0000-4000-8000-000000000001';
const sessionId = '00000000-0000-4000-8000-000000000002';
const transcodeId = '00000000-0000-4000-8000-000000000003';
const replacement = '00000000-0000-4000-8000-000000000004';
const expected = () => ({ resourceId, sessionId, transcodeId, positionMs: 4000, paused: true });
const armed = () => ({ ...expected(), armed: true, encoderIdentity: { pid: 123, parent_pid: 120, started_ticks: 4001 } });
const heartbeat = () => ({ method: 'POST', path: '/api/v1/playback/heartbeat', status: 200, bearer: true, cookie: false,
    request: { session_id: sessionId, transcode_session_id: transcodeId, position_ms: 4000, state: 'paused', is_buffering: false },
    reply: { session_id: sessionId, transcode_session_id: replacement, stream_decision: 'direct_stream', position_ms: 4000, stream_offset_ms: 4000,
        recovery: { status: 'recovered', reason: 'stalled', attempts: 1, skipped_ms: 0, quality_reduced: false } } });

test('native arm binds resource, session, generation, paused checkpoint and exact process identity', () => {
    assert.deepEqual(assertNativeRecoveryArm(armed(), expected()), armed());
    const extra = armed(); extra.encoderIdentity.arguments = 'must not retain process arguments';
    assert.equal(assertNativeRecoveryArm(extra, expected()).encoderIdentity.arguments, undefined);
    for (const change of [
        (proof) => { proof.resourceId = replacement; }, (proof) => { proof.sessionId = replacement; },
        (proof) => { proof.transcodeId = replacement; }, (proof) => { proof.positionMs = 6000; },
        (proof) => { proof.paused = false; }, (proof) => { proof.armed = false; },
        (proof) => { proof.encoderIdentity.pid = 0; }, (proof) => { proof.encoderIdentity.parent_pid = -1; },
        (proof) => { proof.encoderIdentity.started_ticks = Number.MAX_SAFE_INTEGER + 1; },
        (proof) => { proof.encoderIdentity = 'unverified process'; },
    ]) { const proof = armed(); change(proof); assert.throws(() => assertNativeRecoveryArm(proof, expected())); }
});

test('native recovery requires an authenticated natural paused heartbeat and a canonical stalled replacement', () => {
    assert.deepEqual(assertNaturalNativeRecovery(heartbeat(), expected()), heartbeat());
    for (const change of [
        (proof) => { proof.bearer = false; }, (proof) => { proof.cookie = true; },
        (proof) => { proof.status = 401; }, (proof) => { proof.path = '/api/v1/playback/seek'; },
        (proof) => { proof.request.session_id = replacement; }, (proof) => { proof.request.transcode_session_id = replacement; },
        (proof) => { proof.request.position_ms = 4200; }, (proof) => { proof.request.state = 'playing'; },
        (proof) => { proof.request.recovery_requested = true; }, (proof) => { proof.request.recovery_requested = 'false'; },
        (proof) => { delete proof.request.is_buffering; }, (proof) => { proof.reply.session_id = replacement; },
        (proof) => { proof.reply.transcode_session_id = transcodeId; }, (proof) => { proof.reply.transcode_session_id = 'unknown'; },
        (proof) => { proof.reply.stream_decision = 'direct_play'; }, (proof) => { proof.reply.position_ms = 4100; },
        (proof) => { proof.reply.stream_offset_ms = 4100; }, (proof) => { proof.reply.recovery.status = 'exhausted'; },
        (proof) => { proof.reply.recovery.reason = 'decode'; }, (proof) => { proof.reply.recovery.attempts = 0; },
        (proof) => { proof.reply.recovery.attempts = 2; }, (proof) => { proof.reply.recovery.skipped_ms = 4000; },
        (proof) => { proof.reply.recovery.quality_reduced = true; }, (proof) => { delete proof.reply.recovery; },
    ]) { const proof = heartbeat(); change(proof); assert.throws(() => assertNaturalNativeRecovery(proof, expected())); }
    const explicitFalse = heartbeat(); explicitFalse.request.recovery_requested = false;
    assert.doesNotThrow(() => assertNaturalNativeRecovery(explicitFalse, expected()));
});
