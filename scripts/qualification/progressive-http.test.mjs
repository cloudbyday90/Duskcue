// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { validateProgressiveHttpEvidence, validateProgressiveHttpResult } from './progressive-http.mjs';

const resourceId = '11111111-2222-4333-8444-555555555555';
const marker = (value) => `DUSKCUE_PROGRESSIVE_PLAYBACK=${JSON.stringify(value)}\n`;
const evidence = () => ({
    version: 1, executed: true, passed: true, resource_id: resourceId,
    source_seconds: 600.05, source_bytes: 11217150, first_segments: 2, seek_first_segments: 2, completed_segments: 285,
    original_profile_binding_and_refusal: true, seek_released_old_encoder_cache: true,
    stop_released_capacity: true, cleanup_passed: true, fixture_children_confirmed_exited: true,
    encoder_pids: [101, 105, 110, 114], decoded_stream_relative_timestamps: [0.08, 0.125, 0.08, 0.08],
    encoder_identities: [101, 105, 110, 114].map((pid, index) => ({ pid, parent_pid: 20, started_ticks: 1000 + index })),
    encoder_exit_status: 'unqualified_by_http_fixture', browser_to_live_server: 'unqualified',
});

test('source-bound executed HTTP proof preserves all measured fields and explicit unqualified boundaries', () => {
    const value = evidence();
    assert.deepEqual(validateProgressiveHttpResult(`ordinary exact SQL output\r\n${marker(value).replaceAll('\n', '\r\n')}`, resourceId), value);
    assert.deepEqual(validateProgressiveHttpResult(`test stop_contract ... \n${marker(value)}`, resourceId), value);
    assert.throws(() => validateProgressiveHttpResult(`test stop_contract ... ${marker(value)}`, resourceId));
});

test('a successful ordinary SQL result without one current executed marker cannot qualify progressive HTTP', () => {
    for (const output of ['test result: ok. 1 passed; 0 failed;', '', `${marker(evidence())}${marker(evidence())}`, `DUSKCUE_PROGRESSIVE_PLAYBACK=\n${marker(evidence())}`, marker({ ...evidence(), resource_id: 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee' })]) {
        assert.throws(() => validateProgressiveHttpResult(output, resourceId));
    }
    for (const field of ['executed', 'passed', 'original_profile_binding_and_refusal', 'seek_released_old_encoder_cache', 'stop_released_capacity', 'cleanup_passed', 'fixture_children_confirmed_exited']) {
        for (const value of [false, undefined, 'true', 1]) assert.throws(() => validateProgressiveHttpResult(marker({ ...evidence(), [field]: value }), resourceId));
    }
});

test('missing growth, source measurements or actual decoder identities cannot become a progressive pass', () => {
    for (const replacement of [
        { source_seconds: 8 }, { source_seconds: '600' }, { source_seconds: null }, { source_bytes: 0 }, { source_bytes: 33554433 },
        { first_segments: undefined }, { first_segments: 0 }, { seek_first_segments: undefined }, { seek_first_segments: 0 }, { completed_segments: 2 }, { completed_segments: '285' }, { completed_segments: 4097 },
        { encoder_pids: [] }, { encoder_pids: [101, 101, 110, 114] }, { encoder_pids: [101, 105, 110, 0] }, { encoder_identities: [] },
        { decoded_stream_relative_timestamps: [] }, { decoded_stream_relative_timestamps: [0.08, 30.125, 0.08, 0.08] },
        { decoded_stream_relative_timestamps: [0.08, null, 0.08, 0.08] }, { encoder_exit_status: 'succeeded' }, { browser_to_live_server: 'passed' },
    ]) assert.throws(() => validateProgressiveHttpResult(marker({ ...evidence(), ...replacement }), resourceId));
});

test('malformed and oversized markers fail before admission', () => {
    assert.throws(() => validateProgressiveHttpResult('DUSKCUE_PROGRESSIVE_PLAYBACK={invalid}\n', resourceId));
    assert.throws(() => validateProgressiveHttpResult(`DUSKCUE_PROGRESSIVE_PLAYBACK=${' '.repeat(65537)}\n`, resourceId));
    assert.throws(() => validateProgressiveHttpResult(marker(evidence()), 'not-a-resource'));
});

test('host admission validates parsed runtime evidence against its own resource identity', () => {
    assert.deepEqual(validateProgressiveHttpEvidence(evidence(), resourceId), evidence());
    for (const value of [undefined, null, {}, { ...evidence(), fixture_children_confirmed_exited: false }, { ...evidence(), resource_id: 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee' }]) {
        assert.throws(() => validateProgressiveHttpEvidence(value, resourceId));
    }
});

test('growth is measured within the seek replacement instead of comparing different stream generations', () => {
    const value = { ...evidence(), first_segments: 295, seek_first_segments: 2, completed_segments: 286 };
    assert.deepEqual(validateProgressiveHttpResult(marker(value), resourceId), value);
    assert.throws(() => validateProgressiveHttpResult(marker({ ...value, completed_segments: 2 }), resourceId));
});

test('a reused PID needs a distinct recorded creation identity and cannot duplicate an old encoder observation', () => {
    const value = evidence();
    value.encoder_pids[1] = 101;
    value.encoder_identities[1].pid = 101;
    assert.deepEqual(validateProgressiveHttpResult(marker(value), resourceId), value);
    value.encoder_identities[1].started_ticks = value.encoder_identities[0].started_ticks;
    assert.throws(() => validateProgressiveHttpResult(marker(value), resourceId));
});
