// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { assertSourceDecoderRecovery, sourceRecoveryCandidate, sourceRecoveryContract } from './qualification-source-recovery-policy.mjs';
import { installSourceRecoveryErrors } from './qualification-source-recovery-observation.mjs';
import { assertSourceRecoveryStartup, waitForSourceRecoveryStartup } from './qualification-source-recovery-startup.mjs';
import { preserveSourceRecoveryFailure, sourceRecoveryReleaseEvidence } from './qualification-source-recovery-cleanup.mjs';

const item = '11111111-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const file = '22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const session = '33333333-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const generation = '44444444-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const replacement = '55555555-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const revision = 'a'.repeat(64);
const candidate = { item, file, variant: 'video-slice', videoIndex: 1, audioIndex: 2, audioHz: 880, sha256: 'a'.repeat(64) };
const nativeCandidate = { ...candidate, item: '66666666-bbbb-4ccc-8ddd-eeeeeeeeeeee', file: '77777777-bbbb-4ccc-8ddd-eeeeeeeeeeee', variant: 'video-slice-native' };
const error = () => ({ code: 3, trusted: true, generation, source: 'blob:failed', mediaTime: 2.1 });
const heartbeat = () => ({ method: 'POST', path: '/api/v1/playback/heartbeat', status: 200, bearer: true, cookie: false,
    request: { session_id: session, transcode_session_id: generation, position_ms: 2100, recovery_requested: true,
        copy_fallback: { generation, supported: true, smooth: true } },
    reply: { session_id: session, transcode_session_id: replacement, stream_decision: 'transcode', position_ms: 2100, stream_offset_ms: 2100,
        copy_fallback: null, recovery: { status: 'recovered', reason: 'client_decode', attempts: 1, skipped_ms: 0, quality_reduced: true } } });

test('source recovery isolates each client on its owned damaged scanner file with the selected global streams', () => {
    const ready = { sourceRecoveryCandidates: [candidate, nativeCandidate] };
    assert.deepEqual(sourceRecoveryCandidate(ready, 'browser'), candidate);
    assert.deepEqual(sourceRecoveryCandidate(ready, 'native'), nativeCandidate);
    for (const client of [undefined, 'web', 'desktop', 'unknown']) assert.throws(() => sourceRecoveryCandidate(ready, client));
    assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [candidate] }, 'native'));
    assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [candidate, { ...nativeCandidate, item }] }, 'native'));
    assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [candidate, { ...nativeCandidate, file }] }, 'browser'));
    assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [candidate, candidate] }, 'browser'));
    assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [candidate, nativeCandidate, candidate, candidate, candidate] }, 'browser'));
    for (const patch of [{ videoIndex: 0 }, { audioIndex: 0 }, { variant: 'clean' }, { sha256: 'not-a-source' }]) {
        assert.throws(() => sourceRecoveryCandidate({ sourceRecoveryCandidates: [{ ...candidate, ...patch }] }, 'browser'));
    }
});

test('source recovery requires genuine decoder failure plus fresh current-generation supported and smooth admission', () => {
    assert.equal(assertSourceDecoderRecovery([heartbeat()], session, generation, [error()], 'native').generation, replacement);
    for (const mutate of [(value) => { value.request.copy_fallback.smooth = false; },
        (value) => { value.request.copy_fallback.generation = replacement; }, (value) => { value.reply.session_id = item; },
        (value) => { value.reply.recovery.skipped_ms = 4000; }, (value) => { value.reply.recovery.reason = 'stalled'; },
        (value) => { value.reply.recovery.attempts = 2; }, (value) => { value.reply.position_ms += 1; }, (value) => { value.cookie = true; }]) {
        const value = heartbeat(); mutate(value);
        assert.throws(() => assertSourceDecoderRecovery([value], session, generation, [error()], 'native'));
    }
    for (const patch of [{ code: 4 }, { code: 2 }, { trusted: false }, { generation: replacement }, { source: '/not-current' }]) {
        assert.throws(() => assertSourceDecoderRecovery([heartbeat()], session, generation, [{ ...error(), ...patch }], 'native'));
    }
    assert.throws(() => assertSourceDecoderRecovery([heartbeat(), heartbeat()], session, generation, [error()], 'native'));
});

test('source recovery starts bind actual HLS selection to exact preparation, not broad device claims', () => {
    const selection = { media_item_id: nativeCandidate.item, media_file_id: nativeCandidate.file, quality_mode: 'auto', max_streaming_bitrate: null,
        force_hls: true, force_transcode: false, audio_stream_index: null, subtitle_stream_index: null, device_profile: null };
    const configuration = { type: 'media-source', video: { contentType: 'video/mp4; codecs="avc1.640028"', width: 320, height: 180, bitrate: 400000, framerate: 24 },
        audio: { contentType: 'audio/mp4; codecs="mp4a.40.2"', channels: '1', bitrate: 48000, samplerate: 48000 } };
    const records = [{ method: 'GET', path: `/api/v1/items/${nativeCandidate.item}/watch-data`, status: 200, bearer: true, cookie: false,
        watch: { resume_position_ms: 0, is_watched: false }, timing: { startedAtMs: 10, completedAtMs: 18 } },
    { method: 'POST', path: '/api/v1/playback/prepare', status: 200, bearer: true, cookie: false,
        timing: { startedAtMs: 20, completedAtMs: 28 },
        request: { ...selection, capability_protocol: 1 }, reply: { protocol: 1, source_revision: revision,
            candidates: [{ id: 'hls_video_copy_audio_copy', delivery: 'hls', configuration }] } },
    { method: 'POST', path: '/api/v1/playback/start', status: 200, bearer: true, cookie: false,
        timing: { startedAtMs: 30, completedAtMs: 35 },
        request: { ...selection, source_capabilities: { version: 1, source_revision: revision, observations: [{ candidate_id: 'hls_video_copy_audio_copy',
            status: 'reported', supported: true, smooth: true, power_efficient: false, delivery_supported: true }] } },
        reply: { session_id: session, transcode_session_id: generation, stream_decision: 'direct_stream',
            source_route: { candidate_id: 'hls_video_copy_audio_copy', source_revision: revision }, copy_fallback: { generation } } }];
    assert.equal(sourceRecoveryContract(records, nativeCandidate, 'native').route.candidate_id, 'hls_video_copy_audio_copy');
    assert.throws(() => sourceRecoveryContract(records, candidate, 'native'));
    for (const patch of [{ resume_position_ms: 6000 }, { is_watched: true }, { resume_position_ms: undefined }]) {
        const previous = records[0].watch;
        records[0].watch = { ...previous, ...patch };
        assert.throws(() => sourceRecoveryContract(records, nativeCandidate, 'native'));
        records[0].watch = previous;
    }
    assert.throws(() => sourceRecoveryContract(records.slice(1), nativeCandidate, 'native'));
    assert.equal(sourceRecoveryContract([...records.slice(1), records[0]], nativeCandidate, 'native').watch, records[0]);
    for (const timing of [{ startedAtMs: 10, completedAtMs: 21 }, { startedAtMs: 10, completedAtMs: 31 },
        { startedAtMs: 10, completedAtMs: 9 }, { startedAtMs: Number.NaN, completedAtMs: 18 }, null]) {
        const previous = records[0].timing;
        records[0].timing = timing;
        assert.throws(() => sourceRecoveryContract(records, nativeCandidate, 'native'));
        records[0].timing = previous;
    }
    records[2].request.source_capabilities.observations[0].smooth = false;
    assert.throws(() => sourceRecoveryContract(records, nativeCandidate, 'native'));
});

test('passive decoder errors retain native trust and reject a checkpoint after early decoder failure', () => {
    let listener;
    class Media { error = { code: 3 }; currentSrc = 'blob:actual'; currentTime = 2; }
    const context = { window: {}, HTMLVideoElement: Media, performance: { now: () => 100 },
        document: { addEventListener: (_name, callback) => { listener = callback; }, removeEventListener() {} } };
    runInNewContext(`(${installSourceRecoveryErrors.toString()})`, context)();
    context.window.sourceRecoveryErrors.arm(generation);
    listener({ target: new Media(), isTrusted: true });
    const [actual] = context.window.sourceRecoveryErrors.snapshot();
    assert.equal(actual.code, 3); assert.equal(actual.trusted, true); assert.equal(actual.generation, generation);
    assert.throws(() => context.window.sourceRecoveryErrors.arm(replacement));
    context.window.sourceRecoveryErrors.detach();
});

test('source startup refuses repeated successful fetches before they can consume the production burst allowance', async () => {
    const segment = { method: 'GET', path: `/api/v1/transcode/${generation}/source/seg_0000.m4s`, status: 200 };
    assert.doesNotThrow(() => assertSourceRecoveryStartup(Array.from({ length: 8 }, () => segment), generation));
    assert.throws(() => assertSourceRecoveryStartup(Array.from({ length: 9 }, () => segment), generation), /seg_0000\.m4s.*9 successful/);
    assert.throws(() => assertSourceRecoveryStartup([{ ...segment, status: 429 }], generation), /rate limited/);
    let clock = 0;
    let reads = 0;
    await assert.rejects(waitForSourceRecoveryStartup({ generation, readMedia: async () => ({ ready: 0, source: 'blob:loading' }),
        readRecords: () => Array.from({ length: ++reads * 3 }, () => segment), now: () => clock,
        wait: async (milliseconds) => { clock += milliseconds; } }), /9 successful/);
    assert.equal(reads, 3);
    assert.equal(clock, 200);
});

test('source startup retains the five-second ready-media deadline and honors cancellation', async () => {
    let clock = 0;
    await assert.rejects(waitForSourceRecoveryStartup({ generation, readMedia: async () => ({ ready: 0, source: 'blob:loading' }),
        readRecords: () => [], now: () => clock, wait: async (milliseconds) => { clock += milliseconds; } }), /five seconds/);
    assert.equal(clock, 5000);
    const controller = new AbortController();
    controller.abort();
    await assert.rejects(waitForSourceRecoveryStartup({ generation, signal: controller.signal, readRecords: () => { throw new Error('Late records must not be read'); } }), /canceled/);
    assert.deepEqual(await waitForSourceRecoveryStartup({ generation, readRecords: () => [], readMedia: async () => ({ ready: 2, source: 'blob:ready' }) }), { ready: 2, source: 'blob:ready' });
});

test('preference restoration cannot replace the original media failure with a cleanup rate-limit error', async () => {
    const media = new Error('Repeated startup segment before any healthy frame');
    const cleanup = new Error('Rate limit exceeded');
    await assert.rejects(preserveSourceRecoveryFailure(async () => { throw media; }, async () => { throw cleanup; }), (error) => {
        assert.ok(error instanceof AggregateError);
        assert.equal(error.cause, media);
        assert.deepEqual(error.errors, [media, cleanup]);
        assert.match(error.message, /Repeated startup segment.*restoration also failed.*Rate limit/);
        return true;
    });
    let restored = false;
    await assert.rejects(preserveSourceRecoveryFailure(async () => { throw media; }, async () => { restored = true; }), (error) => error === media);
    assert.equal(restored, true);
    assert.equal(await preserveSourceRecoveryFailure(async () => 7, async () => {}), 7);
});

test('client cleanup distinguishes no allocated source from an unreleased allocated session without claiming physical retirement', () => {
    const none = sourceRecoveryReleaseEvidence([]);
    assert.equal(none.passed, true);
    assert.deepEqual(none.allocatedSessions, []);
    assert.equal(none.physicalCleanupAuthority, 'isolated-backend-final-cleanup');
    const start = { path: '/api/v1/playback/start', status: 200, reply: { session_id: session } };
    const stopped = { path: '/api/v1/playback/stop', status: 200, request: { session_id: session } };
    assert.equal(sourceRecoveryReleaseEvidence([start, stopped]).clientPlaybackReleased, true);
    const failed = sourceRecoveryReleaseEvidence([start, { ...stopped, status: 429 }]);
    assert.equal(failed.passed, false);
    assert.equal(failed.clientPlaybackReleased, false);
    assert.deepEqual(failed.unreleasedSessions, [session]);
    assert.equal(sourceRecoveryReleaseEvidence([start, stopped], ['audio']).passed, false);
});
