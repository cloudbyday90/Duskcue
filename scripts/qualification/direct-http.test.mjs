// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { validatePlaybackDirectResult } from './direct-http.mjs';

const resource = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const timing = { timestamp_alignment: true, video_start_seconds: 0.066, audio_start_seconds: 0.045, av_start_delta_seconds: 0.021 };
function backend(native = false) {
    return { same_session: true, old_generation_retired: true, source_offset_ms: native ? 4000 : 5000,
        selected_video_index: 1, selected_audio_index: 2, attempts: native ? 0 : 1, output_profile: native ? 'ordinary_copy' : 'compatible_sdr', transport: native ? 'bearer' : 'cookie',
        authorization: { unauthenticated_status: 401, peer_status: 404, kids_status: 404 },
        actual_media: { ...timing, decoded_timestamp: 0.066, selected_audio_hz: 880, video_profile: 'High', video_level: 40,
            video_frame_rate: native ? '24/1' : '30/1', audio_sample_rate: '48000', audio_channels: 1 } };
}
function evidence() {
    return { version: 1, resource_id: resource, executed: true, passed: true, cleanup_passed: true, fixture_children_confirmed_exited: true,
        source_seconds: 8, source_bytes: 500000, contracts: { revision_and_profile_fencing: true, direct_seek_rotated_revision: true,
            concurrent_promotion_one_attempt_same_session: true, stop_released_owned_generation: true, capability_and_changed_unhealthy_refusal_without_workers: true,
            rollback_cancelled_unpublished_worker: true, legacy_client_bookkeeping_without_promotion: true, original_source_sha256: 'a'.repeat(64), clean_source_sha256: 'b'.repeat(64),
            encoded_media: { ...timing, decoded_timestamp: 0.066667, selected_audio_hz: 880, video_profile: 'High', video_level: 40, video_frame_rate: '30/1', audio_sample_rate: '48000', audio_channels: 1 } },
        browser: { resourceId: resource, passed: true, unauthenticatedMediaStatus: 401, nativeError: { code: 3, trusted: true }, directFrames: 10, hlsFrames: 5, selectedAudioHz: 878.9, selectedAudioDb: -35, hlsElapsedMs: 750,
            fullPlayer: true, productionRouter: true, closedThroughUi: true, recoveryNoticeVisible: true, recoveryNotice: 'A recovery stream is ready at reduced playback quality.',
            postSeek: { paused: true, recovery: { attempts: 1 }, selectedAudioIndex: 2, generationFrames: 6, firstFrameMediaTime: 0.066, frameMediaTime: 0.8 },
            production_policy: { production_router: true, request_id: true, nosniff: true, unauthenticated_start_status: 401, non_json_start_status: 415, credentialed_cross_origin_allowed: false },
            backend: { browser: backend(), native: backend(true) }, native: { resourceId: resource, passed: true, pageErrors: 0,
                checks: { authenticatedHls: true, closedCanonicalTitleAndStoppedOnce: true, seek: { preservedPause: true }, backend: { passed: true } },
                credentialCleanup: { exactOwnedCredentialCleared: true }, processCleanup: { status: 'passed' } } } };
}
const marker = (value) => `DUSKCUE_PLAYBACK_DIRECT=${JSON.stringify(value)}\n`;
test('direct qualification binds executed contracts, exact media, cleanup and the current resource', () => {
    assert.deepEqual(validatePlaybackDirectResult(marker(evidence()), resource, true), evidence());
    for (const change of [
        (value) => { value.executed = false; }, (value) => { value.cleanup_passed = false; },
        (value) => { value.resource_id = 'foreign'; }, (value) => { value.contracts.rollback_cancelled_unpublished_worker = false; },
        (value) => { value.contracts.legacy_client_bookkeeping_without_promotion = false; },
        (value) => { value.contracts.encoded_media.video_level = 42; }, (value) => { value.contracts.encoded_media.selected_audio_hz = 220; },
        (value) => { value.contracts.encoded_media.decoded_timestamp = 1.9; }, (value) => { value.contracts.original_source_sha256 = ''; },
    ]) { const value = evidence(); change(value); assert.throws(() => validatePlaybackDirectResult(marker(value), resource, true)); }
    assert.throws(() => validatePlaybackDirectResult('', resource));
    assert.throws(() => validatePlaybackDirectResult(marker(evidence()) + marker(evidence()), resource));
});
test('live-client proof rejects a lost budget, stale generation, missing UI notice or unowned native cleanup', () => {
    assert.doesNotThrow(() => validatePlaybackDirectResult(marker(evidence()), resource, true, true));
    for (const change of [
        (v) => { v.browser.backend.browser.attempts = 0; }, (v) => { v.browser.backend.browser.old_generation_retired = false; },
        (v) => { v.browser.recoveryNoticeVisible = false; }, (v) => { v.browser.postSeek.firstFrameMediaTime = 2; },
        (v) => { v.browser.production_policy.non_json_start_status = 200; }, (v) => { v.browser.native.credentialCleanup.exactOwnedCredentialCleared = false; },
        (v) => { v.browser.native.processCleanup.status = 'failed'; }, (v) => { v.browser.backend.native.actual_media.selected_audio_hz = 220; },
        (v) => { v.browser.backend.native.authorization.kids_status = 200; },
        (v) => { v.browser.backend.native.actual_media.av_start_delta_seconds = 0.2; },
        (v) => { v.contracts.encoded_media.timestamp_alignment = false; },
    ]) { const value = evidence(); change(value); assert.throws(() => validatePlaybackDirectResult(marker(value), resource, true, true)); }
});
test('simulated error events, absent frames and the wrong audio cannot qualify real-browser recovery', () => {
    for (const change of [
        (value) => { value.browser.nativeError.trusted = false; }, (value) => { value.browser.nativeError.code = 4; },
        (value) => { value.browser.directFrames = 0; }, (value) => { value.browser.hlsFrames = 0; },
        (value) => { value.browser.selectedAudioHz = 220; }, (value) => { value.browser.resourceId = 'foreign'; },
        (value) => { value.browser.unauthenticatedMediaStatus = 200; },
        (value) => { value.browser.selectedAudioDb = -100; }, (value) => { value.browser.hlsElapsedMs = 100; },
    ]) { const value = evidence(); change(value); assert.throws(() => validatePlaybackDirectResult(marker(value), resource, true)); }
    const value = evidence(); value.browser = null;
    assert.doesNotThrow(() => validatePlaybackDirectResult(marker(value), resource, false));
    assert.throws(() => validatePlaybackDirectResult(marker(value), resource, true));
});

function pacedEvidence() {
    const value = evidence();
    const session = '11111111-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const stalled = '22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const recovered = '33333333-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const final = '44444444-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const decoded = { frames: 8, currentTime: 0.8, width: 320, height: 180 };
    const audio = { frequency: 880, level: -35, elapsedMs: 750, running: true };
    const identity = { pid: 123, parent_pid: 12, started_ticks: 456 };
    const checkpoint = { ...backend(true), attempts: 1, native_recovery: { reason: 'stalled', attempts: 1, skipped_ms: 0, quality_reduced: false, paused: true,
        frozen_encoder_gone: true, stalled_cache_retired: true, encoder_identity: identity, stalled_generation: stalled, recovered_generation: recovered } };
    value.browser.backend['native-recovery'] = checkpoint;
    value.browser.backend.native = { ...backend(true), attempts: 1, source_offset_ms: 6000 };
    value.browser.native.requests = [
        { method: 'POST', path: '/api/v1/playback/start', status: 200, bearer: true, cookie: false, request: { force_hls: true }, reply: { session_id: session, transcode_session_id: stalled, stream_decision: 'direct_stream' } },
        ...[recovered, final].flatMap((generation) => ['manifest.m3u8', 'seg_0000.m4s'].map((path) => ({ method: 'GET', path: `/api/v1/transcode/${generation}/${path}`, status: 200, bearer: true, cookie: false }))),
        ...[[4000, stalled], [6000, final]].map(([position, generation]) => ({ method: 'POST', path: '/api/v1/playback/seek', status: 200, bearer: true, cookie: false,
            request: { session_id: session, position_ms: position }, reply: { session_id: session, position_ms: position, transcode_session_id: generation } })),
    ];
    value.browser.native.checks.recovery = {
        armed: { resourceId: resource, sessionId: session, transcodeId: stalled, positionMs: 4000, paused: true, armed: true, encoderIdentity: identity },
        heartbeat: { method: 'POST', path: '/api/v1/playback/heartbeat', status: 200, bearer: true, cookie: false,
            request: { session_id: session, transcode_session_id: stalled, position_ms: 4000, state: 'paused', is_buffering: false },
            reply: { session_id: session, transcode_session_id: recovered, position_ms: 4000, stream_offset_ms: 4000, stream_decision: 'direct_stream',
                recovery: { status: 'recovered', reason: 'stalled', attempts: 1, skipped_ms: 0, quality_reduced: false } } },
        sessionId: session, previousTranscodeId: stalled, transcodeId: recovered, positionMs: 4000, attempts: 1, preservedPause: true,
        unexpectedPlayEvents: 0, notice: 'A more compatible playback stream is ready.', freshBlobSource: true, backend: { passed: true }, decoded, audio,
    };
    value.browser.native.checks.seek = { positionMs: 6000, preservedPause: true, freshBlobSource: true, transcodeId: final, decoded, audio };
    value.browser.native.requests.push(value.browser.native.checks.recovery.heartbeat);
    return value;
}

test('paced native proof requires natural stalled recovery, physical retirement and fresh post-recovery presentation', () => {
    assert.doesNotThrow(() => validatePlaybackDirectResult(marker(pacedEvidence()), resource, true, true, true));
    assert.throws(() => validatePlaybackDirectResult(marker(pacedEvidence()), resource, true, true, false));
    assert.throws(() => validatePlaybackDirectResult(marker(evidence()), resource, true, true, true));
    for (const change of [
        (v) => { delete v.browser.backend['native-recovery']; },
        (v) => { v.browser.native.checks.recovery.armed.encoderIdentity.started_ticks = 0; },
        (v) => { v.browser.native.checks.recovery.heartbeat.request.recovery_requested = true; },
        (v) => { v.browser.native.checks.recovery.heartbeat.request.state = 'playing'; },
        (v) => { v.browser.native.checks.recovery.unexpectedPlayEvents = 1; },
        (v) => { v.browser.native.checks.recovery.notice = ''; },
        (v) => { v.browser.native.checks.recovery.audio.frequency = 220; },
        (v) => { v.browser.native.checks.recovery.decoded.currentTime = 0; },
        (v) => { v.browser.native.checks.seek.positionMs = 4000; },
        (v) => { v.browser.backend['native-recovery'].native_recovery.frozen_encoder_gone = false; },
        (v) => { v.browser.backend['native-recovery'].native_recovery.stalled_cache_retired = false; },
        (v) => { v.browser.backend['native-recovery'].native_recovery.quality_reduced = true; },
        (v) => { v.browser.native.requests.push(v.browser.native.requests[0]); },
        (v) => { v.browser.native.requests = v.browser.native.requests.filter((record) => record.path !== '/api/v1/playback/heartbeat'); },
        (v) => { v.browser.native.requests = v.browser.native.requests.filter((record) => record.path !== '/api/v1/playback/seek'); },
        (v) => { v.browser.native.requests.find((record) => record.path === '/api/v1/playback/seek').reply.transcode_session_id = 'foreign'; },
    ]) { const value = structuredClone(pacedEvidence()); change(value); assert.throws(() => validatePlaybackDirectResult(marker(value), resource, true, true, true)); }
});
