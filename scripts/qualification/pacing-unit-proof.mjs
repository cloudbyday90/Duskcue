// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const PACING_UNIT_CASES = Object.freeze([
    'ordinary_input_assembly_is_unchanged_without_qualification_policy',
    'pacing_is_exact_source_scoped_and_does_not_leak_between_managers',
    'owned_input_rejects_aliases_and_non_owned_or_non_regular_paths',
    'owned_input_enforces_nonempty_sixteen_mib_limit',
    'replaced_or_redirected_configured_source_refuses_input_assembly',
    'changed_source_refuses_real_start_before_cache_or_worker_allocation',
    'same_length_in_place_edit_invalidates_the_admitted_identity',
    'restarts_reselect_pacing_without_changing_tracks_profile_or_attempts',
]);

export function validatePacingUnitProof(output) {
    const lines = output.split(/\r?\n/);
    for (const name of PACING_UNIT_CASES) if (lines.filter((line) => line === `test services::transcoding::qualification::tests::${name} ... ok`).length !== 1) throw new Error('An executed owned-input pacing regression is missing.');
    if (!lines.some((line) => /^test result: ok\. 8 passed; 0 failed; 0 ignored;/.test(line))) throw new Error('The eight pacing regressions did not all execute successfully.');
    return { passed: true, cases: PACING_UNIT_CASES.length, execution: 'Linux qualification compiler stage; bounded file-only unit cases, separate from native runtime proof.' };
}
