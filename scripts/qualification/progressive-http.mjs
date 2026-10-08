// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function validateProgressiveHttpResult(stdout, resourceId) {
    if (typeof stdout !== 'string' || stdout.length > 16777216 || !/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(resourceId || '')) throw new Error('Progressive HTTP result needs bounded output and the current resource identity.');
    let record;
    for (const match of stdout.matchAll(/^DUSKCUE_PROGRESSIVE_PLAYBACK=(.*)$/gm)) {
        if (record !== undefined || match[1].length > 65536) throw new Error('Exactly one bounded executed progressive HTTP marker is required.');
        record = match[1];
    }
    if (record === undefined) throw new Error('Exactly one bounded executed progressive HTTP marker is required.');
    return validateProgressiveHttpEvidence(JSON.parse(record), resourceId);
}

export function validateProgressiveHttpEvidence(result, resourceId) {
    if (!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(resourceId || '')) throw new Error('Progressive HTTP result needs the current resource identity.');
    if (!result || result.version !== 1 || result.resource_id !== resourceId || result.executed !== true || result.passed !== true) throw new Error('Progressive HTTP execution is absent, failed or belongs to another resource.');
    for (const field of ['original_profile_binding_and_refusal', 'seek_released_old_encoder_cache', 'stop_released_capacity', 'cleanup_passed', 'fixture_children_confirmed_exited']) {
        if (result[field] !== true) throw new Error(`Progressive HTTP proof is incomplete: ${field}.`);
    }
    if (!Number.isFinite(result.source_seconds) || Math.abs(result.source_seconds - 600) > 1
        || !Number.isSafeInteger(result.source_bytes) || result.source_bytes <= 0 || result.source_bytes > 33554432) throw new Error('Progressive HTTP source size/runtime is unavailable or outside its fixture bounds.');
    if (!Number.isSafeInteger(result.first_segments) || result.first_segments < 1 || result.first_segments > 4096
        || !Number.isSafeInteger(result.seek_first_segments) || result.seek_first_segments < 1
        || !Number.isSafeInteger(result.completed_segments) || result.completed_segments <= result.seek_first_segments || result.completed_segments > 4096) throw new Error('Progressive HTTP publication did not prove bounded segment growth within the replacement generation.');
    if (!Array.isArray(result.encoder_pids) || result.encoder_pids.length !== 4
        || result.encoder_pids.some((pid) => !Number.isSafeInteger(pid) || pid < 1 || pid > 4294967295)) throw new Error('All four exact observed encoder identities are required.');
    if (!Array.isArray(result.encoder_identities) || result.encoder_identities.length !== 4
        || result.encoder_identities.some((identity, index) => !identity || identity.pid !== result.encoder_pids[index]
            || !Number.isSafeInteger(identity.parent_pid) || identity.parent_pid < 1 || identity.parent_pid > 4294967295
            || !Number.isSafeInteger(identity.started_ticks) || identity.started_ticks < 1)
        || new Set(result.encoder_identities.map((identity) => `${identity.pid}:${identity.parent_pid}:${identity.started_ticks}`)).size !== 4) throw new Error('All four distinct pinned encoder creation identities are required.');
    if (!Array.isArray(result.decoded_stream_relative_timestamps) || result.decoded_stream_relative_timestamps.length !== 4
        || result.decoded_stream_relative_timestamps.some((value) => !Number.isFinite(value) || value < 0 || value > 0.25)) throw new Error('All four physically decoded near-zero stream timestamps are required.');
    if (result.encoder_exit_status !== 'unqualified_by_http_fixture' || result.browser_to_live_server !== 'unqualified') throw new Error('Progressive HTTP evidence must preserve its actual exit-status and browser proof boundaries.');
    return result;
}
