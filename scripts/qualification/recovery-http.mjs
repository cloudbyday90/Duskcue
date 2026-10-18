// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function validatePlaybackRecoveryResult(stdout, resourceId) {
    if (typeof stdout !== 'string' || stdout.length > 16777216 || !/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(resourceId || '')) throw new Error('Recovery proof requires bounded output and the owning resource identity.');
    const markers = [...stdout.matchAll(/^DUSKCUE_PLAYBACK_RECOVERY=(.*)$/gm)];
    if (markers.length !== 1 || markers[0][1].length > 65536) throw new Error('Exactly one bounded executed recovery marker is required.');
    const result = JSON.parse(markers[0][1]);
    if (result.version !== 1 || result.executed !== true || result.passed !== true || result.resource_id !== resourceId) throw new Error('Recovery proof is absent, failed or belongs to another workflow.');
    for (const field of ['real_process_stall', 'healthy_hint_did_not_restart', 'generation_replay_preserved_position_budget', 'seek_and_stop_serialized_with_heartbeat', 'recovery_kept_original_profile', 'cleanup_passed', 'fixture_children_confirmed_exited']) {
        if (result[field] !== true) throw new Error(`Recovery proof is incomplete: ${field}.`);
    }
    if (!Number.isFinite(result.source_seconds) || Math.abs(result.source_seconds - 600) > 1
        || !Number.isSafeInteger(result.source_bytes) || result.source_bytes <= 0 || result.source_bytes > 33554432) throw new Error('Recovery source escaped its duration or size bounds.');
    if (!Array.isArray(result.encoder_identities) || result.encoder_identities.length !== 3
        || result.encoder_identities.some((identity) => !identity || ['pid', 'parent_pid', 'started_ticks'].some((field) => !Number.isSafeInteger(identity[field]) || identity[field] < 1))
        || new Set(result.encoder_identities.map((identity) => `${identity.pid}:${identity.parent_pid}:${identity.started_ticks}`)).size !== 3) throw new Error('Three distinct pinned recovery encoder identities are required.');
    if (!Array.isArray(result.decoded_stream_relative_timestamps) || result.decoded_stream_relative_timestamps.length !== 3
        || result.decoded_stream_relative_timestamps.some((value) => !Number.isFinite(value) || value < 0 || value > 0.25)
        || result.browser_to_live_server !== 'unqualified') throw new Error('Recovery proof requires three physically decoded stream-relative timestamps and its actual browser boundary.');
    return result;
}
