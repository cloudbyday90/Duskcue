// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { validateNativeRecoveryProof } from './native-recovery-proof.mjs';

export function validatePlaybackDirectResult(stdout, resourceId, requireBrowser = false, requireNative = false, requireNativeRecovery = false) {
    if (requireNativeRecovery && !requireNative) throw new Error('Native recovery qualification requires the actual native client.');
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
    validateTiming(media);
    if (requireBrowser && (result.browser?.resourceId !== resourceId || result.browser.passed !== true || result.browser.unauthenticatedMediaStatus !== 401
        || result.browser.nativeError?.code !== 3 || result.browser.nativeError.trusted !== true
        || !Number.isSafeInteger(result.browser.directFrames) || result.browser.directFrames < 1
        || !Number.isSafeInteger(result.browser.hlsFrames) || result.browser.hlsFrames < 5
        || !Number.isFinite(result.browser.selectedAudioDb) || result.browser.selectedAudioDb <= -60
        || !Number.isFinite(result.browser.hlsElapsedMs) || result.browser.hlsElapsedMs < 500
        || !Number.isFinite(result.browser.selectedAudioHz) || Math.abs(result.browser.selectedAudioHz - 880) >= 20)) throw new Error('Actual authenticated native-to-HLS browser decoding was not established.');
    if (requireBrowser) {
        const browser = result.browser;
        if (browser.fullPlayer !== true || browser.productionRouter !== true || browser.closedThroughUi !== true
            || browser.recoveryNoticeVisible !== true || browser.recoveryNotice !== 'A recovery stream is ready at reduced playback quality.'
            || browser.postSeek?.paused !== true || browser.postSeek.recovery?.attempts !== 1 || browser.postSeek.selectedAudioIndex !== 2
            || !Number.isSafeInteger(browser.postSeek.generationFrames) || browser.postSeek.generationFrames < 5 || !Number.isFinite(browser.postSeek.firstFrameMediaTime)
            || browser.postSeek.firstFrameMediaTime < 0 || browser.postSeek.firstFrameMediaTime > 0.25
            || !(browser.postSeek.frameMediaTime > browser.postSeek.firstFrameMediaTime)
            || browser.production_policy?.production_router !== true || browser.production_policy.request_id !== true || browser.production_policy.nosniff !== true
            || browser.production_policy.unauthenticated_start_status !== 401 || browser.production_policy.non_json_start_status !== 415
            || browser.production_policy.credentialed_cross_origin_allowed !== false) throw new Error('Actual Player UI, seek, timing and production middleware proof is incomplete.');
        validateLiveBackend(browser.backend?.browser, 'compatible_sdr', 1, 5000, '30/1');
    }
    if (requireNative) {
        const native = result.browser?.native;
        if (!requireBrowser || native?.resourceId !== resourceId || native.passed !== true || native.pageErrors !== 0
            || native.checks?.authenticatedHls !== true || native.checks.closedCanonicalTitleAndStoppedOnce !== true
            || native.credentialCleanup?.exactOwnedCredentialCleared !== true || native.processCleanup?.status !== 'passed'
            || native.checks.seek?.preservedPause !== true || native.checks.backend?.passed !== true) throw new Error('Actual bearer desktop playback or owned cleanup proof is incomplete.');
        validateLiveBackend(result.browser.backend?.native, 'ordinary_copy', requireNativeRecovery ? 1 : 0, requireNativeRecovery ? 6000 : 4000, '24/1');
        if (requireNativeRecovery) {
            const checkpoint = result.browser.backend?.['native-recovery'];
            validateLiveBackend(checkpoint, 'ordinary_copy', 1, 4000, '24/1');
            validateNativeRecoveryProof(native, checkpoint, resourceId);
        }
    }
    return result;
}

function validateLiveBackend(backend, profile, attempts, offset, frameRate) {
    if (backend?.same_session !== true || backend.old_generation_retired !== true || backend.source_offset_ms !== offset
        || backend.selected_video_index !== 1 || backend.selected_audio_index !== 2 || backend.attempts !== attempts || backend.output_profile !== profile
        || backend.transport !== (profile === 'compatible_sdr' ? 'cookie' : 'bearer')
        || backend.authorization?.unauthenticated_status !== 401 || backend.authorization.peer_status !== 404 || backend.authorization.kids_status !== 404
        || backend.actual_media?.video_profile !== 'High' || backend.actual_media.video_level !== 40 || backend.actual_media.video_frame_rate !== frameRate
        || backend.actual_media.audio_sample_rate !== '48000' || backend.actual_media.audio_channels !== 1
        || !Number.isFinite(backend.actual_media.decoded_timestamp) || backend.actual_media.decoded_timestamp < 0 || backend.actual_media.decoded_timestamp > 0.25
        || !Number.isFinite(backend.actual_media.selected_audio_hz) || Math.abs(backend.actual_media.selected_audio_hz - 880) >= 20) throw new Error('Live seek did not preserve admitted output, selected audio, timing, budget or authorization.');
    validateTiming(backend.actual_media);
}

function validateTiming(media) {
    if (media.timestamp_alignment !== true || !Number.isFinite(media.video_start_seconds) || !Number.isFinite(media.audio_start_seconds)
        || media.video_start_seconds < 0 || media.video_start_seconds > 0.25 || media.audio_start_seconds < 0 || media.audio_start_seconds > 0.25
        || !Number.isFinite(media.av_start_delta_seconds) || Math.abs(media.av_start_delta_seconds) > 0.1
        || Math.abs(media.av_start_delta_seconds - (media.video_start_seconds - media.audio_start_seconds)) > 0.000001) throw new Error('Muxed audio/video start timestamps do not establish bounded alignment.');
}
