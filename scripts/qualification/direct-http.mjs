// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function validatePlaybackDirectResult(stdout, resourceId, requireBrowser = false) {
    const records = stdout.split(/\r?\n/).filter((line) => line.startsWith('DUSKCUE_PLAYBACK_DIRECT='));
    if (records.length !== 1) throw new Error('One executed direct-fallback contract marker is required.');
    const result = JSON.parse(records[0].slice('DUSKCUE_PLAYBACK_DIRECT='.length));
    if (result.version !== 1 || result.resource_id !== resourceId || result.executed !== true || result.passed !== true
        || result.cleanup_passed !== true || result.fixture_children_confirmed_exited !== true
        || result.source_seconds !== 8 || !Number.isSafeInteger(result.source_bytes) || result.source_bytes <= 0 || result.source_bytes > 16777216) throw new Error('Direct-fallback execution or cleanup proof is incomplete.');
    for (const field of ['revision_and_profile_fencing', 'direct_seek_rotated_revision', 'concurrent_promotion_one_attempt_same_session', 'stop_released_owned_generation', 'capability_and_changed_unhealthy_refusal_without_workers', 'rollback_cancelled_unpublished_worker', 'legacy_client_bookkeeping_without_promotion']) {
        if (result.contracts?.[field] !== true) throw new Error(`Missing direct contract: ${field}.`);
    }
    for (const field of ['original_source_sha256', 'clean_source_sha256']) if (!/^[a-f0-9]{64}$/.test(result.contracts[field] || '')) throw new Error('Direct source identity is missing.');
    const media = result.contracts.encoded_media;
    if (!Number.isFinite(media?.decoded_timestamp) || media.decoded_timestamp < 0 || media.decoded_timestamp > 0.25
        || !Number.isFinite(media.selected_audio_hz) || Math.abs(media.selected_audio_hz - 880) >= 20
        || media.video_profile !== 'High' || media.video_level !== 40 || media.video_frame_rate !== '30/1'
        || media.audio_sample_rate !== '48000' || media.audio_channels !== 1) throw new Error('Actual compatible output differs from its admitted descriptor.');
    if (requireBrowser && (result.browser?.resourceId !== resourceId || result.browser.passed !== true || result.browser.unauthenticatedMediaStatus !== 401
        || result.browser.nativeError?.code !== 3 || result.browser.nativeError.trusted !== true
        || !Number.isSafeInteger(result.browser.directFrames) || result.browser.directFrames < 1
        || !Number.isSafeInteger(result.browser.hlsFrames) || result.browser.hlsFrames < 5
        || !Number.isFinite(result.browser.selectedAudioDb) || result.browser.selectedAudioDb <= -60
        || !Number.isFinite(result.browser.hlsElapsedMs) || result.browser.hlsElapsedMs < 500
        || !Number.isFinite(result.browser.selectedAudioHz) || Math.abs(result.browser.selectedAudioHz - 880) >= 20)) throw new Error('Actual authenticated native-to-HLS browser decoding was not established.');
    return result;
}
