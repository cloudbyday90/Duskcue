// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';

export function validateSourceUnitProof(output) {
    const names = [
        'services::source_capabilities::tests::avc_requires_actual_complete_parameter_records_and_matching_sps_identity',
        'workers::library_scanner::probe::capabilities::tests::pinned_ffprobe_newline_wrapped_av1_configuration_is_accepted_without_guessing_raw_obus',
        'workers::library_scanner::scan_diff::tests::quick_scan_reprobes_new_resized_and_subsecond_modified_files',
        'domains::playback::source_candidates::tests::exact_positive_support_smoothness_and_delivery_are_required_independently_of_power',
        'domains::playback::direct_admission::tests::device_network_and_request_limits_reserve_audio_from_the_same_total_budget',
        'domains::playback::source_freshness::tests::current_file_must_match_both_scanner_snapshot_and_database_precision',
        'domains::playback::copy_recovery::tests::decode_fallback_acknowledges_only_the_current_generation_with_positive_smooth_support',
        'domains::playback::source_types::tests::supported_reports_have_finite_bounded_typed_observations',
        'domains::playback::file_delivery::tests::cancelled_file_body_releases_its_actual_descriptor',
        'domains::playback::range::tests::handles_empty_and_maximum_sizes_without_arithmetic_overflow',
        'services::transcoding::readiness::source_origin::tests::earliest_decode_timestamp_matches_hls_normalization_across_timescales',
        'services::transcoding::output_profile::source_copy::tests::copied_hevc_keeps_dimensions_and_only_converted_audio_gets_encoder_options',
        'extractors::credential_transport_tests::authenticated_transport_uses_the_same_cookie_first_token_selection',
    ];
    for (const name of names) assert.equal(output.split(/\r?\n/).filter((line) => line === `test ${name} ... ok`).length, 1, name);
    const summaries = [...output.matchAll(/^test result: ok\. (\d+) passed; 0 failed; 0 ignored;/gm)];
    assert.equal(summaries.length, names.length);
    assert.ok(summaries.every((summary) => Number(summary[1]) > 0));
    return { passed: true, groups: summaries.length, cases: summaries.reduce((total, summary) => total + Number(summary[1]), 0), execution: 'Current Linux feature artifact; exact source admission, timing, scanner and file descriptor tests' };
}

export function validateSourceSummaries(browser, manifest) {
    assert.equal(manifest.version, 1);
    const sourceIds = manifest.cases.map((candidate) => candidate.id);
    assert.equal(sourceIds.length, 14);
    assert.equal(new Set(sourceIds).size, 14);
    const clients = [];
    for (const [summary, client] of [[browser.sourceCapabilities, 'browser'], [browser.native?.checks?.sourceCapabilities, 'native']]) {
        assert.equal(summary?.passed, true);
        assert.equal(summary.client, client);
        assert.equal(summary.cases, 14);
        assert.equal(summary.played, 10);
        assert.equal(summary.refused, 4);
        assert.deepEqual(summary.sourceIds, sourceIds);
        assert.ok(Number.isInteger(summary.optimized) && summary.optimized >= 1 && summary.optimized <= 10);
        assert.ok(Number.isInteger(summary.conservative) && summary.conservative >= 0);
        assert.equal(summary.optimized + summary.conservative, 10);
        const recovery = client === 'browser' ? browser.sourceRecovery : browser.native?.checks?.sourceRecovery;
        assert.equal(recovery?.passed, true);
        assert.equal(recovery.client, client);
        assert.equal(recovery.decoderCode, 3);
        assert.equal(recovery.genuine, true);
        assert.equal(recovery.attempts, 1);
        assert.equal(recovery.positionMs, 5000);
        assert.equal(recovery.healthyCopy, true);
        assert.equal(recovery.backend, true);
        assert.equal(recovery.cleanup, true);
        clients.push({ client, optimized: summary.optimized, conservative: summary.conservative });
    }
    return { passed: true, cases: 14, clients, played: 20, refused: 8, genuineCopyRecoveries: 2, sourceIds };
}

export function validateSourceBackend(backend) {
    assert.equal(backend?.executed, true);
    assert.equal(backend.source_capabilities, true);
    assert.equal(backend.source_descriptors?.length, 14);
    validateCopyBoundary(backend.source_copy_boundary);
    const scan = backend.quick_scan;
    for (const field of ['passed', 'actual_scanner', 'all_inventory_identities_preserved', 'descriptor_and_hash_preserved', 'actual_source_mtime_restored', 'source_sha256_unchanged', 'sources_unchanged']) assert.equal(scan?.[field], true, field);
    assert.equal(scan.stale_mtime_ms, 1);
    assert.equal(scan.unchanged_files_skipped, 14);
    assert.equal(scan.reprobed_files, 1);
    assert.equal(scan.updated_files, 1);
    for (const [counts, modified, unchanged, updated] of [[scan.unchanged, 0, 14, 0], [scan.stale_mtime, 1, 13, 1]]) {
        assert.deepEqual(counts, { type: 'quick', discovered: 14, new: 0, modified, unchanged, deleted: 0, updated, errors: 0 });
    }
    const recovery = backend.source_recovery;
    for (const field of ['executed', 'scanner_ingested', 'genuine_decoder_failure', 'sources_unchanged', 'cleanup_passed']) assert.equal(recovery?.[field], true, field);
    assert.equal(recovery.clients, 2);
    const damage = recovery.authored_damage;
    assert.equal(damage?.damaged_samples, 48);
    assert.ok(Number.isSafeInteger(damage.damaged_vcl_nals) && damage.damaged_vcl_nals >= 48 && damage.damaged_vcl_nals <= 3072);
    assert.equal(damage.idr_headers_preserved, 1);
    assert.equal(damage.damage_start_ms, 2000);
    assert.equal(damage.damage_end_ms, 4000);
    for (const field of ['avcc_lengths_headers_and_non_vcl_preserved', 'source_unchanged', 'client_items_isolated', 'native_bytes_equal_browser']) assert.equal(damage[field], true, field);
    assert.equal(damage.initial_watch_rows, 0);
    assert.deepEqual(Object.keys(recovery.results).sort(), ['source-recovery-browser', 'source-recovery-native']);
    for (const client of ['browser', 'native']) {
        const result = recovery.results[`source-recovery-${client}`];
        assert.equal(result.transport, client === 'browser' ? 'cookie' : 'bearer');
        assert.equal(result.variant, client === 'browser' ? 'video-slice' : 'video-slice-native');
        assert.match(result.file, /^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/i);
        assert.equal(result.attempts, 1);
        assert.equal(result.skipped_ms, 0);
        assert.equal(result.output_profile, 'compatible_sdr');
        const negative = result.negative_seek;
        assert.equal(negative?.same_user_kids_status, 404);
        assert.equal(negative.unhealthy_source_status, 404);
        assert.equal(negative.error_code, 'PLAY_001');
        for (const field of ['canonical_generation_preserved', 'requested_checkpoint_not_written', 'no_seek_event', 'recovery_budget_preserved', 'no_generation_allocated', 'healthy_worker_and_cache_preserved', 'file_health_and_identity_restored']) assert.equal(negative[field], true, field);
    }
    assert.notEqual(recovery.results['source-recovery-browser'].file, recovery.results['source-recovery-native'].file);
    return { passed: true, actualQuickScans: 2, reprobedFiles: 1, refusedSeeks: 4, copyBoundaryCases: 2, canonicalGenerationsPreserved: true };
}

function validateCopyBoundary(boundary) {
    for (const field of ['passed', 'actual_manager', 'source_preserved', 'reference_preserved', 'cleanup_passed']) assert.equal(boundary?.[field], true, field);
    for (const field of ['source_sha256', 'reference_sha256']) assert.match(boundary[field], /^[a-f0-9]{64}$/);
    assert.ok(Number.isInteger(boundary.source_b_frames) && boundary.source_b_frames > 0);
    assert.equal(boundary.cases?.length, 2);
    assert.deepEqual(boundary.cases.map((item) => item.audio_action).sort(), ['copy', 'encode']);
    for (const item of boundary.cases) {
        assert.equal(item.source_offset_ms, item.audio_action === 'copy' ? 3904 : 3916);
        for (const field of ['no_edit_lists', 'exact_source_pixels', 'cleanup_passed']) assert.equal(item[field], true, field);
        assert.equal(item.source_frame_id, 120);
        assert.equal(item.source_video_index, 1);
        assert.equal(item.source_audio_index, 2);
        assert.equal(item.output_profile, 'source_copy_sdr');
        assert.match(item.native_pixels_sha256, /^[a-f0-9]{64}$/);
        assert.ok(Number.isFinite(item.selected_audio_hz) && Math.abs(item.selected_audio_hz - 880) < 20);
        assert.equal(item.track_dts?.length, 2);
        assert.deepEqual(item.track_dts.map((track) => track.handler).sort(), ['soun', 'vide']);
        assert.equal(new Set(item.track_dts.map((track) => track.track_id)).size, 2);
        for (const track of item.track_dts) {
            assert.ok(Number.isSafeInteger(track.track_id) && track.track_id > 0);
            assert.ok(Number.isSafeInteger(track.timescale) && track.timescale > 0 && track.timescale <= 1e9);
            assert.ok(Number.isSafeInteger(track.tfdt) && track.tfdt >= 0);
            assert.equal(track.floor_ms, Number(BigInt(track.tfdt) * 1000n / BigInt(track.timescale)));
        }
        assert.equal(item.source_offset_ms, Math.min(...item.track_dts.map((track) => track.floor_ms)));
    }
}
