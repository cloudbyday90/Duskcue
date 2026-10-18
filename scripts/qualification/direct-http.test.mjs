// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { validatePlaybackDirectResult } from './direct-http.mjs';

const resource = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
function evidence() {
    return { version: 1, resource_id: resource, executed: true, passed: true, cleanup_passed: true, fixture_children_confirmed_exited: true,
        source_seconds: 8, source_bytes: 500000, contracts: { revision_and_profile_fencing: true, direct_seek_rotated_revision: true,
            concurrent_promotion_one_attempt_same_session: true, stop_released_owned_generation: true, capability_and_changed_unhealthy_refusal_without_workers: true,
            rollback_cancelled_unpublished_worker: true, legacy_client_bookkeeping_without_promotion: true, original_source_sha256: 'a'.repeat(64), clean_source_sha256: 'b'.repeat(64),
            encoded_media: { decoded_timestamp: 0.066667, selected_audio_hz: 880, video_profile: 'High', video_level: 40, video_frame_rate: '30/1', audio_sample_rate: '48000', audio_channels: 1 } },
        browser: { resourceId: resource, passed: true, unauthenticatedMediaStatus: 401, nativeError: { code: 3, trusted: true }, directFrames: 10, hlsFrames: 5, selectedAudioHz: 878.9, selectedAudioDb: -35, hlsElapsedMs: 750 } };
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
