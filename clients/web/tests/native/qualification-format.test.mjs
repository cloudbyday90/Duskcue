// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { queryFormatCapability } from '../fixtures/format-capabilities.mjs';
import { assertFormatCases, assertFormatMedia, assertFormatPaused, assertFormatPresentation, assertFormatSeek, assertFormatStart, formatPauseReady, formatSeeks, initialFormatGeneration } from './qualification-format-policy.mjs';

const item = '11111111-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const file = '22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const session = '33333333-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const generation = '44444444-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const capability = { type: 'file', video: { contentType: 'video/mp4; codecs="av01.0.04M.10"', width: 320, height: 180, bitrate: 100000, framerate: 24 },
    audio: { contentType: 'audio/mp4; codecs="opus"', channels: '1', bitrate: 48000, samplerate: 48000 } };
function cases() {
    return ['hevc-sdr-8', 'hevc-sdr-10', 'av1-sdr-8', 'av1-sdr-10', 'hevc-pq-10', 'hevc-hlg-10'].flatMap((master) =>
        (master.startsWith('av1') ? ['mp4', 'mkv', 'webm'] : ['mp4', 'mkv']).map((container) => ({ id: `${master}-${container}`, master, container,
            item, file, codec: master.startsWith('av1') ? 'av1' : 'hevc', bitDepth: master.endsWith('-8') ? 8 : 10,
            dynamicRange: master.includes('-pq-') ? 'hdr10' : master.includes('-hlg-') ? 'hlg' : 'sdr',
            videoIndex: 0, audioIndex: 2, audioHz: 880, sha256: 'a'.repeat(64), capability }))).map((entry, index) => ({ ...entry, index }));
}
const start = () => ({ method: 'POST', path: '/api/v1/playback/start', status: 200, bearer: true, cookie: false,
    request: { media_item_id: item, media_file_id: file, device_profile: null, force_transcode: false, force_hls: true, quality_mode: 'manual', max_streaming_bitrate: 1500000, audio_stream_index: null, subtitle_stream_index: null },
    reply: { session_id: session, transcode_session_id: generation, stream_decision: 'transcode' } });

test('format cases bind all fourteen exact source identities and conservative native starts', () => {
    assert.equal(assertFormatCases(cases()).length, 14);
    for (const mutate of [(v) => v.pop(), (v) => { v[1].id = v[0].id; }, (v) => { v[0].bitDepth = 10; },
        (v) => { v[0].audioIndex = 1; }, (v) => { v[13].dynamicRange = 'sdr'; }, (v) => { v[0].index = 2; }]) {
        const value = structuredClone(cases()); mutate(value); assert.throws(() => assertFormatCases(value));
    }
    assert.doesNotThrow(() => assertFormatStart(start(), cases()[0], 'native'));
    for (const mutate of [(v) => { v.request.device_profile = {}; }, (v) => { v.cookie = true; }, (v) => { v.request.force_transcode = true; },
        (v) => { v.request.audio_stream_index = 2; }, (v) => { v.reply.stream_decision = 'direct_play'; }]) {
        const value = start(); mutate(value); assert.throws(() => assertFormatStart(value, cases()[0], 'native'));
    }
});

test('format evidence refuses stale presentation and incomplete authenticated delivery', () => {
    const frames = { generation, source: 'blob:owned', currentSource: 'blob:owned', frames: 12, ready: 4, width: 320, height: 180, firstMediaTime: 0.04, lastMediaTime: 0.7 };
    assert.doesNotThrow(() => assertFormatPresentation(frames, generation));
    for (const patch of [{ generation: session }, { currentSource: 'blob:old' }, { frames: 0 }, { lastMediaTime: 0.1 }, { width: 0 }]) assert.throws(() => assertFormatPresentation({ ...frames, ...patch }, generation));
    const media = ['manifest.m3u8', 'init.mp4', 'seg_0000.m4s'].map((path) => ({ method: 'GET', path: `/api/v1/transcode/${generation}/source/${path}`, status: 200, bearer: true, cookie: false }));
    assert.doesNotThrow(() => assertFormatMedia(media, generation, 'native'));
    assert.throws(() => assertFormatMedia(media.slice(1), generation, 'native'));
    assert.throws(() => assertFormatMedia(media.map((record) => ({ ...record, bearer: false })), generation, 'native'));
});

test('format pause intent rejects loading-time paused media and mismatched controls', () => {
    const paused = { ready: 2, paused: true, ended: false, playVisible: true, pauseVisible: false };
    assert.doesNotThrow(() => assertFormatPaused(paused));
    for (const patch of [{ ready: 0 }, { paused: false }, { ended: true }, { playVisible: false }, { pauseVisible: true }]) assert.throws(() => assertFormatPaused({ ...paused, ...patch }));
    const ended = { ...paused, ended: true };
    assert.equal(formatPauseReady(ended), false);
    assert.equal(formatPauseReady(ended, true), true);
    assert.equal(formatPauseReady({ ...ended, ready: 0 }, true), false);
    assert.equal(formatPauseReady({ ...ended, playVisible: false }, true), false);
    assert.throws(() => assertFormatPaused(ended));
});

test('format HDR proof requires the explicit refusal rather than a generic process failure', () => {
    const candidate = { ...cases()[10], expectedStatus: 422, expectedCode: 'PLAY_020' };
    const record = { ...start(), status: 422, reply: undefined, error: { code: 'PLAY_020', detail: 'HDR conversion is not supported on this device yet.' } };
    assert.doesNotThrow(() => assertFormatStart(record, candidate, 'native'));
    for (const patch of [{ status: 500 }, { error: { code: 'PLAY_004', detail: 'FFmpeg process failed' } }, { error: { code: 'PLAY_020', detail: 'Something failed' } }]) {
        assert.throws(() => assertFormatStart({ ...record, ...patch }, candidate, 'native'));
    }
});

test('format resume ownership uses the exact saved position and rejects missing, foreign or additional rotations', () => {
    const resumed = '55555555-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const watch = { method: 'GET', path: `/api/v1/items/${item}/watch-data`, status: 200, bearer: true, cookie: false,
        watch: { resume_position_ms: 5909, is_watched: false } };
    const automatic = { method: 'POST', path: '/api/v1/playback/seek', status: 200, bearer: true, cookie: false,
        request: { session_id: session, position_ms: 5909 }, reply: { session_id: session, position_ms: 5909, transcode_session_id: resumed } };
    const records = [watch, start(), automatic];
    assert.deepEqual(initialFormatGeneration(records, start(), cases()[0], 'native'), { resumePositionMs: 5909, generation: resumed, automaticSeek: automatic, seekCount: 1 });
    for (const mutate of [(v) => v.pop(), (v) => v.push(structuredClone(automatic)), (v) => { v[2].request.session_id = file; },
        (v) => { v[2].reply.position_ms = 5000; }, (v) => { v[2].reply.transcode_session_id = generation; }, (v) => { v[0].watch.resume_position_ms = 6000; }]) {
        const value = structuredClone(records); mutate(value); assert.throws(() => initialFormatGeneration(value, start(), cases()[0], 'native'));
    }
    const fresh = [{ ...watch, watch: { resume_position_ms: 0, is_watched: false } }, start()];
    assert.equal(initialFormatGeneration(fresh, start(), cases()[0], 'native').generation, generation);
    assert.equal(formatSeeks(records).length, 1);
    const reset = { ...automatic, request: { session_id: session, position_ms: 0 }, reply: { session_id: session, position_ms: 0, transcode_session_id: file } };
    assert.equal(assertFormatSeek(reset, { sessionId: session, previousTranscodeId: resumed, positionMs: 0, client: 'native' }), file);
    assert.throws(() => assertFormatSeek(automatic, { sessionId: session, previousTranscodeId: resumed, positionMs: 5000, client: 'native' }));
});

test('format diagnostics preserve three independent reports and distinct unavailable, rejected and timeout outcomes', async () => {
    const execute = (decodingInfo, timeout = false) => runInNewContext(`(${queryFormatCapability.toString()})`, {
        navigator: decodingInfo ? { mediaCapabilities: { decodingInfo } } : {}, MediaSource: { isTypeSupported: () => true },
        setTimeout: timeout ? (callback) => { queueMicrotask(callback); return 1; } : setTimeout, clearTimeout,
    })(capability);
    const reported = await execute(async () => ({ supported: true, smooth: false, powerEfficient: false }));
    assert.equal(reported.status, 'reported'); assert.equal(reported.supported, true); assert.equal(reported.smooth, false); assert.equal(reported.powerEfficient, false);
    assert.deepEqual(JSON.parse(JSON.stringify(reported.configuration)), capability);
    assert.equal((await execute(null)).status, 'unavailable');
    const rejected = await execute(async () => { throw new TypeError('private diagnostic content'); });
    assert.equal(rejected.status, 'rejected'); assert.equal(rejected.error, 'TypeError'); assert.equal(JSON.stringify(rejected).includes('private diagnostic content'), false);
    assert.equal((await execute(() => new Promise(() => {}), true)).status, 'timed_out');
});
