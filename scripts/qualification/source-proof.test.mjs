// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { validateSourceBackend, validateSourceSummaries } from './source-proof.mjs';
import { validateSourceReport } from '../../docker/qualification/direct-format-control.mjs';

const resource = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';

function copyBoundary() {
    return { passed: true, actual_manager: true, source_preserved: true, reference_preserved: true, cleanup_passed: true, source_sha256: 'a'.repeat(64), reference_sha256: 'b'.repeat(64), source_b_frames: 2,
        cases: ['copy', 'encode'].map((audio_action) => ({ audio_action, source_offset_ms: audio_action === 'copy' ? 3904 : 3916, no_edit_lists: true, exact_source_pixels: true, cleanup_passed: true, source_frame_id: 120, source_video_index: 1, source_audio_index: 2, output_profile: 'source_copy_sdr', native_pixels_sha256: 'c'.repeat(64), selected_audio_hz: 880,
            track_dts: [{ track_id: 1, handler: 'vide', timescale: 12288, tfdt: 48128, floor_ms: 3916 }, { track_id: 2, handler: 'soun', timescale: 48000, tfdt: audio_action === 'copy' ? 187392 : 238976, floor_ms: audio_action === 'copy' ? 3904 : 4978 }] })) };
}

test('source backend proof requires actual quick scanning and refused seeks in both credential lanes', () => {
    const negative = { same_user_kids_status: 404, unhealthy_source_status: 404, error_code: 'PLAY_001', canonical_generation_preserved: true, requested_checkpoint_not_written: true, no_seek_event: true, recovery_budget_preserved: true, no_generation_allocated: true, healthy_worker_and_cache_preserved: true, file_health_and_identity_restored: true };
    const counts = { type: 'quick', discovered: 14, new: 0, modified: 0, unchanged: 14, deleted: 0, updated: 0, errors: 0 };
    const result = (transport) => ({ transport, variant: transport === 'cookie' ? 'video-slice' : 'video-slice-native',
        file: transport === 'cookie' ? resource : 'bbbbbbbb-bbbb-4ccc-8ddd-eeeeeeeeeeee',
        attempts: 1, skipped_ms: 0, output_profile: 'compatible_sdr', negative_seek: { ...negative } });
    const backend = { executed: true, source_capabilities: true, source_descriptors: Array(14).fill({}), source_copy_boundary: copyBoundary(),
        quick_scan: { passed: true, actual_scanner: true, all_inventory_identities_preserved: true, descriptor_and_hash_preserved: true, actual_source_mtime_restored: true, source_sha256_unchanged: true, sources_unchanged: true, stale_mtime_ms: 1, unchanged_files_skipped: 14, reprobed_files: 1, updated_files: 1, unchanged: counts, stale_mtime: { ...counts, modified: 1, unchanged: 13, updated: 1 } },
        source_recovery: { executed: true, scanner_ingested: true, genuine_decoder_failure: true, sources_unchanged: true, cleanup_passed: true, clients: 2,
            authored_damage: { damaged_samples: 48, damaged_vcl_nals: 48, idr_headers_preserved: 1, damage_start_ms: 2000, damage_end_ms: 4000, avcc_lengths_headers_and_non_vcl_preserved: true, source_unchanged: true,
                client_items_isolated: true, native_bytes_equal_browser: true, initial_watch_rows: 0 },
            results: { 'source-recovery-browser': result('cookie'), 'source-recovery-native': result('bearer') } } };
    assert.equal(validateSourceBackend(backend).refusedSeeks, 4);
    for (const mutate of [
        (copy) => { delete copy.quick_scan; },
        (copy) => { copy.quick_scan.stale_mtime.modified = 0; },
        (copy) => { copy.quick_scan.actual_source_mtime_restored = false; },
        (copy) => { delete copy.source_recovery.results['source-recovery-native']; },
        (copy) => { delete copy.source_recovery.authored_damage; },
        (copy) => { copy.source_recovery.results['source-recovery-native'].file = copy.source_recovery.results['source-recovery-browser'].file; },
        (copy) => { copy.source_recovery.authored_damage.damaged_samples = 0; },
        (copy) => { copy.source_recovery.authored_damage.avcc_lengths_headers_and_non_vcl_preserved = false; },
        (copy) => { copy.source_recovery.results['source-recovery-browser'].negative_seek.requested_checkpoint_not_written = false; },
        (copy) => { copy.source_recovery.results['source-recovery-native'].negative_seek.healthy_worker_and_cache_preserved = false; },
        (copy) => { delete copy.source_copy_boundary; },
        (copy) => { copy.source_copy_boundary.cases[0].source_offset_ms = 0; },
        (copy) => { copy.source_copy_boundary.cases[1].exact_source_pixels = false; },
        (copy) => { copy.source_copy_boundary.cases[0].track_dts[1].tfdt = 0; },
        (copy) => { copy.source_copy_boundary.cases[1].cleanup_passed = false; },
    ]) {
        const copy = structuredClone(backend);
        mutate(copy);
        assert.throws(() => validateSourceBackend(copy));
    }
});

test('source qualification independently requires both actual optimized and conservative matrix accounting', () => {
    const sourceIds = Array.from({ length: 14 }, (_, index) => `source-${index}`);
    const summary = (client) => ({ passed: true, cases: 14, played: 10, refused: 4, client, sourceIds, optimized: 5, conservative: 5 });
    const recovery = (client) => ({ passed: true, client, decoderCode: 3, genuine: true, attempts: 1, positionMs: 5000, healthyCopy: true, backend: true, cleanup: true });
    const browser = { sourceCapabilities: summary('browser'), sourceRecovery: recovery('browser'), native: { checks: { sourceCapabilities: summary('native'), sourceRecovery: recovery('native') } } };
    const manifest = { version: 1, cases: sourceIds.map((id) => ({ id })) };
    assert.equal(validateSourceSummaries(browser, manifest).passed, true);
    for (const mutate of [
        (copy) => { copy.sourceCapabilities.optimized = 0; copy.sourceCapabilities.conservative = 10; },
        (copy) => { copy.native.checks.sourceCapabilities.conservative = 4; },
        (copy) => { copy.native.checks.sourceCapabilities.sourceIds = [...sourceIds].reverse(); },
        (copy) => { copy.sourceCapabilities.refused = 0; },
        (copy) => { copy.native.checks.sourceRecovery.genuine = false; },
        (copy) => { copy.sourceRecovery.attempts = 2; },
    ]) {
        const copy = structuredClone(browser);
        mutate(copy);
        assert.throws(() => validateSourceSummaries(copy, manifest));
    }
});

test('source verification markers cannot cross resources lanes indices or completed reports', () => {
    const report = { resourceId: resource, phase: 'source-browser-13' };
    assert.equal(validateSourceReport(report, resource, true, new Set()), report.phase);
    for (const phase of ['source-native-00', 'source-recovery-browser', 'source-recovery-native', 'source-recovery-armed-browser', 'source-recovery-armed-native']) {
        assert.equal(validateSourceReport({ ...report, phase }, resource, true, new Set()), phase);
    }
    for (const phase of ['format-browser-00', 'source-browser-14', 'source-native-1', 'source-foreign-00', '../source-browser-00']) {
        assert.throws(() => validateSourceReport({ ...report, phase }, resource, true, new Set()));
    }
    assert.throws(() => validateSourceReport(report, resource, false, new Set()));
    assert.throws(() => validateSourceReport(report, 'foreign', true, new Set()));
    assert.throws(() => validateSourceReport(report, resource, true, new Set([report.phase])));
});
