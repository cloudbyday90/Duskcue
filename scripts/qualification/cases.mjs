// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const REQUIRED_MANAGED_CASES = Object.freeze([
    'services::transcoding::launcher::tests::actual_production_managed_ffmpeg_writes_a_finite_completed_hls_playlist',
    'services::transcoding::launcher::tests::actual_active_managed_ffmpeg_releases_child_before_cache_and_permit',
    'services::sandbox::owned_output::tests::actual_mandatory_bootstrap_denies_later_exec_network_and_outside_paths',
    'services::sandbox::owned_output::tests::actual_constructor_refuses_missing_corrupt_and_wrong_arch_policy_before_main',
    'services::sandbox::owned_output::tests::abandoned_managed_observer_terminates_after_stdout_eof_before_output_cleanup',
    'services::sandbox::owned_output::tests::actual_short_managed_output_survives_delayed_bootstrap_observation',
]);

export const PRODUCER_CASE = 'services::transcoding::arguments::tests::real_audio_first_default_description_and_selected_srt_decode_through_production_arguments';
export const REQUIRED_UNIT_PREFIXES = Object.freeze([
    'services::transcoding::readiness::tests::',
    'services::transcoding::lifecycle::readiness_tests::',
    'services::transcoding::lifecycle::tests::',
    'services::sandbox::wire::tests::',
    'services::sandbox::descriptors::tests::',
    'services::sandbox::managed_elf::tests::',
    'services::sandbox::filter::tests::',
]);

export function selectedUnitCases(inventory) {
    const selected = inventory.filter((test) => REQUIRED_UNIT_PREFIXES.some((prefix) => test.startsWith(prefix)));
    for (const prefix of REQUIRED_UNIT_PREFIXES) {
        if (!selected.some((test) => test.startsWith(prefix))) throw new Error(`The current library artifact has no ${prefix} cases.`);
    }
    if (selected.some((test) => !/^[A-Za-z0-9_:]+$/.test(test))) throw new Error('The library artifact contains an invalid test selector.');
    return selected.sort();
}
