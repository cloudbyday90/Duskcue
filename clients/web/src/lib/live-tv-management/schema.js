/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { instant } from '../time/instant.js';

export function requireValue(value) { if (!value) throw new Error('Invalid management metadata'); }
export function id(value) { requireValue(typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value)); return value; }
export function integer(value, min = 0, max = 2147483647) { requireValue(Number.isSafeInteger(value) && value >= min && value <= max); return value; }
export function flag(value) { requireValue(typeof value === 'boolean'); return value; }
export function label(value) { requireValue(typeof value === 'string' && [...value].length <= 200); return value; }
export function revision(value) { requireValue(typeof value === 'string' && /^[1-9]\d{0,18}$/.test(value) && BigInt(value) <= 9223372036854775807n); return value; }
export function cap(value) { return value === null ? null : integer(value, 1); }
export function timestamp(value) { if (value === null) return null; instant(value); return value; }
export function etag(value) { requireValue(typeof value === 'string' && /^"[0-9a-f]{64}"$/.test(value)); return value; }

const exactKeys = new Set(['revision', 'source_generation', 'assignment_revision', 'latest_attempt_sequence', 'attempt_sequence']);
export function exactReviver(key, value, context = undefined) {
    if (!exactKeys.has(key) || value === null) return value;
    requireValue(typeof value === 'number');
    const source = context?.source;
    if (typeof source === 'string') return revision(source);
    requireValue(Number.isSafeInteger(value));
    return revision(String(value));
}

export function assignment(value, household, owner = false) {
    requireValue(value && value.household_user_id === household && value.configured === true && value.management_scope === (owner ? 'household_owner' : 'administrator'));
    const result = { assignment_id: id(value.assignment_id), household_user_id: id(value.household_user_id), revision: revision(value.revision), source_generation: revision(value.source_generation), management_scope: value.management_scope };
    for (const key of ['enabled', 'allow_owner_profile_access', 'allow_owner_live_limit', 'network_checked', 'playback_available']) result[key] = flag(value[key]);
    for (const key of ['max_live_streams', 'owner_max_live_streams', 'configured_live_tv_cap']) result[key] = cap(value[key]);
    for (const feed of ['playlist', 'guide']) {
        requireValue(['http', 'https'].includes(value[`${feed}_transport`])); result[`${feed}_transport`] = value[`${feed}_transport`];
        result[`${feed}_address_count`] = integer(value[`${feed}_address_count`], 0, 8);
        result[`${feed}_destination_approved`] = flag(value[`${feed}_destination_approved`]);
        requireValue(result[`${feed}_destination_approved`] === (result[`${feed}_address_count`] > 0));
    }
    return result;
}
export function control(value) { return { id: id(value?.id), revision: revision(value.revision), enabled: flag(value.enabled) }; }
export function resource(value, expected) {
    requireValue(value?.assignment_id === expected.assignment_id && value.revision === expected.revision && value.source_generation === expected.source_generation);
    return { assignment_id: id(value.assignment_id), revision: revision(value.revision), source_generation: revision(value.source_generation), assignment_enabled: flag(value.assignment_enabled),
        playlist_address_count: integer(value.playlist_address_count, 0, 8), guide_address_count: integer(value.guide_address_count, 0, 8), control: control(value.control),
        latest_attempt_id: value.latest_attempt_id === null ? null : id(value.latest_attempt_id), latest_attempt_sequence: value.latest_attempt_sequence === null ? null : revision(value.latest_attempt_sequence) };
}
export function profiles(value, expected) {
    requireValue(value?.assignment_revision === expected.revision && Array.isArray(value.items) && value.items.length <= 20);
    return { assignment_revision: revision(value.assignment_revision), next_cursor: value.next_cursor === null ? null : id(value.next_cursor), items: value.items.map((p) => {
        requireValue(['standard', 'kids'].includes(p.profile_type));
        return { id: id(p.id), name: label(p.name), profile_type: p.profile_type, enabled: flag(p.enabled), eligible: flag(p.eligible) };
    }) };
}
export function users(value, page) {
    requireValue(value && value.page === page && value.page_size === 20 && Array.isArray(value.items) && value.items.length <= 20);
    return { page: integer(value.page, 1), total_pages: integer(value.total_pages), items: value.items.map((u) => ({ id: id(u.id), username: label(u.username), display_name: u.display_name === null ? '' : label(u.display_name) })) };
}
export function status(value, expected = null) {
    requireValue(value && ['never_imported', 'not_current', 'unfinished_uncertain', 'current'].includes(value.observation));
    requireValue(value.expected_attempt_id === expected);
    let latest = null;
    if (value.latest !== null) {
        const v = value.latest;
        requireValue(['running', 'succeeded', 'failed', 'indeterminate'].includes(v.state));
        requireValue(v.category === null || ['authority', 'capacity', 'cancelled', 'deadline', 'network', 'format', 'limit', 'storage', 'indeterminate', 'cleanup'].includes(v.category));
        latest = { assignment_id: id(v.assignment_id), attempt_id: id(v.attempt_id), attempt_sequence: revision(v.attempt_sequence), state: v.state, category: v.category,
            attempted_at: timestamp(v.attempted_at), finished_at: timestamp(v.finished_at), imported_at: timestamp(v.imported_at), snapshot_id: v.snapshot_id === null ? null : id(v.snapshot_id),
            channel_count: integer(v.channel_count), guide_count: integer(v.guide_count), programme_count: integer(v.programme_count), current_source: flag(v.current_source) };
        if (value.observation === 'current' && expected) requireValue(latest.attempt_id === expected);
    }
    return { latest, expected_attempt_id: expected, observation: value.observation, locally_owned: flag(value.locally_owned), service_available: flag(value.service_available) };
}
export function accepted(value, location, household) {
    const attempt_id = id(value?.attempt_id); requireValue(attempt_id[14] === '7');
    const path = `/api/v1/live-tv/households/${household}/refresh/status?attempt_id=${attempt_id}`;
    requireValue(value.status_url === path && location === path);
    return { attempt_id, attempt_sequence: revision(value.attempt_sequence) };
}
export function parseCap(value) { requireValue(value === '' || /^[1-9]\d{0,9}$/.test(value)); return value === '' ? null : integer(Number(value), 1); }
