// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { assertSourceMedia, assertSourcePresentation, assertSourceSeek, assertSourceStart, initialSourceOwnership } from './qualification-source-policy.mjs';
import { installSourcePresentation } from './qualification-source-presentation.mjs';

const item = '11111111-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const file = '22222222-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const session = '33333333-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const owner = '44444444-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const nextOwner = '55555555-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const revision = 'a'.repeat(64);
const candidate = { item, file, dynamicRange: 'sdr' };
const configuration = { type: 'file', video: { contentType: 'video/mp4; codecs="av01.0.00M.10"', width: 320, height: 180, bitrate: 75000, framerate: 24 },
    audio: { contentType: 'audio/mp4; codecs="opus"', channels: '1', bitrate: 48000, samplerate: 48000 } };
function records(native = true) {
    const id = native ? 'native_file' : 'hls_video_copy_aac';
    const selection = { media_item_id: item, media_file_id: file, quality_mode: 'auto', max_streaming_bitrate: null,
        device_profile: null, force_transcode: false, audio_stream_index: null, subtitle_stream_index: null, ...(!native ? { force_hls: true } : {}) };
    const auth = { bearer: !native, cookie: native };
    const preparation = { protocol: 1, source_revision: revision,
        candidates: [{ id, delivery: native ? 'file' : 'hls', configuration: { ...configuration, type: native ? 'file' : 'media-source' } }] };
    const observations = { version: 1, source_revision: revision, observations: [{ candidate_id: id, status: 'reported',
        supported: true, smooth: true, power_efficient: false, delivery_supported: true }] };
    return [{ method: 'POST', path: '/api/v1/playback/prepare', status: 200, ...auth, request: { ...selection, capability_protocol: 1 }, reply: preparation },
        { method: 'POST', path: '/api/v1/playback/start', status: 200, ...auth, request: { ...selection, source_capabilities: observations },
            reply: { session_id: session, stream_decision: native ? 'direct_play' : 'transcode', transcode_session_id: native ? null : owner,
                direct_source_revision: native ? owner : null, source_route: { candidate_id: id, source_revision: revision } } }];
}

test('source routing binds positive support and smoothness to exact prepared candidate/transport/source', () => {
    assert.equal(assertSourceStart(records(), candidate, 'browser').native, true);
    assert.equal(assertSourceStart(records(false), candidate, 'native').native, false);
    for (const mutate of [(v) => { v[1].request.source_capabilities.source_revision = 'b'.repeat(64); },
        (v) => { v[1].request.source_capabilities.observations[0].smooth = false; }, (v) => { v[1].reply.source_route.candidate_id = 'hls_video_copy_aac'; },
        (v) => { v[0].cookie = false; }, (v) => { v[1].request.audio_stream_index = 2; }, (v) => { v[1].request.quality_mode = 'manual'; }]) {
        const value = records(); mutate(value); assert.throws(() => assertSourceStart(value, candidate, 'browser'));
    }
    const conservative = records(false);
    conservative[1].request.source_capabilities.observations[0].smooth = false;
    conservative[1].reply.source_route = null;
    assert.equal(assertSourceStart(conservative, candidate, 'native').optimized, false);
});

test('source HDR evidence requires real422 and no proposed optimized candidates', () => {
    const value = records();
    value[0].reply.candidates = [];
    value[1].request.source_capabilities.observations = [];
    value[1].status = 422;
    value[1].reply = undefined;
    value[1].error = { code: 'PLAY_020' };
    assert.equal(assertSourceStart(value, { ...candidate, dynamicRange: 'hdr10' }, 'browser').optimized, false);
    value[1].status = 500;
    assert.throws(() => assertSourceStart(value, { ...candidate, dynamicRange: 'hdr10' }, 'browser'));
});

test('direct resume uses actual native revision and explicit UI seek rotates revision without worker', () => {
    const value = records();
    value.unshift({ method: 'GET', path: `/api/v1/items/${item}/watch-data`, status: 200, bearer: false, cookie: true,
        watch: { resume_position_ms: 5909, is_watched: false } });
    const contract = assertSourceStart(value, candidate, 'browser');
    assert.deepEqual(initialSourceOwnership(value, contract, candidate, 'browser'), { positionMs: 5909, owner, automaticSeek: null, seekCount: 0 });
    const seek = { method: 'POST', path: '/api/v1/playback/seek', status: 200, bearer: false, cookie: true,
        request: { session_id: session, position_ms: 5000 }, reply: { session_id: session, position_ms: 5000, direct_source_revision: nextOwner, transcode_session_id: null } };
    assert.equal(assertSourceSeek(seek, { sessionId: session, previousOwner: owner, positionMs: 5000, client: 'browser', native: true }), nextOwner);
    assert.throws(() => assertSourceSeek(seek, { sessionId: session, previousOwner: nextOwner, positionMs: 5000, client: 'browser', native: true }));
    value.push(seek);
    assert.throws(() => initialSourceOwnership(value, contract, candidate, 'browser'));
});

test('source delivery and first presentation reject wrong authentication, retired owners and visible preroll', () => {
    const media = [{ method: 'GET', path: `/api/v1/stream/${file}`, status: 206, bearer: false, cookie: true }];
    assert.doesNotThrow(() => assertSourceMedia(media, candidate, owner, 'browser', true));
    assert.throws(() => assertSourceMedia(media, candidate, owner, 'native', true));
    const frames = { owner, native: false, source: 'blob:owned', currentSource: 'blob:owned', ready: 4, width: 320, height: 180,
        frames: 15, firstMediaTime: 1.084, lastMediaTime: 1.7 };
    const expected = { owner, native: false, width: 320, height: 180, targetPositionMs: 5000, streamOffsetMs: 3916 };
    assert.doesNotThrow(() => assertSourcePresentation(frames, expected));
    assert.throws(() => assertSourcePresentation({ ...frames, firstMediaTime: 0.084 }, expected));
    assert.throws(() => assertSourcePresentation({ ...frames, owner: nextOwner }, expected));
    assert.throws(() => assertSourcePresentation({ ...frames, currentSource: 'blob:retired' }, expected));
});

test('passive source presentation retains frames before generation attribution instead of hiding preroll', () => {
    const listeners = new Map();
    let callback;
    class Media {
        currentSrc = 'blob:owned'; readyState = 4; videoWidth = 320; videoHeight = 180; currentTime = 0; paused = true; ended = false;
        requestVideoFrameCallback(value) { callback = value; return 1; }
        cancelVideoFrameCallback() {}
        addEventListener(name, value) { listeners.set(name, value); }
        removeEventListener(name) { listeners.delete(name); }
    }
    const video = new Media();
    const context = { window: {}, document: { querySelector: () => video }, HTMLVideoElement: Media,
        performance: { now: () => 100 }, location: { origin: 'http://owned.test' }, URL };
    runInNewContext(`(${installSourcePresentation.toString()})`, context)();
    callback(10, { mediaTime: 0.084 });
    context.window.sourcePresentation.bind({ owner, native: false, fileId: file });
    callback(20, { mediaTime: 1.084 });
    assert.equal(context.window.sourcePresentation.snapshot().firstMediaTime, 0.084);
    context.window.sourcePresentation.detach();
    assert.equal(listeners.size, 0);
});
