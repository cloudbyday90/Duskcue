// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export const ambientStages = Object.freeze(['ambient-background', 'ambient-controls', 'ambient-advance',
    'ambient-cold-retain', 'ambient-cold-restore', 'ambient-scope', 'ambient-concurrency']);
const flags = Object.freeze({
    'ambient-background': ['native_http_tls', 'negative_tls', 'owned_probe_completion', 'background_renewal'],
    'ambient-controls': ['paused_renewal', 'same_viewer_seek', 'paused_intent'],
    'ambient-advance': ['natural_advance', 'replacement_cleanup'],
    'ambient-cold-retain': ['retained_for_process_death'],
    'ambient-cold-restore': ['cleanup_only_restore', 'missed_lease_refused'],
    'ambient-scope': ['profile_cleanup_only', 'strong_scope_clear'],
    'ambient-concurrency': ['independent_interactive'],
});
export function ambientFlags(scenario) {
    if (!ambientStages.includes(scenario)) throw new Error('Unknown ambient stage.');
    return ['native_owner', ...flags[scenario], ...scenario.endsWith('-retain') ? [] : ['confirmed_cleanup']];
}
