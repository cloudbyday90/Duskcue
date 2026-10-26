/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { instant } from './instant.js';

const encoder = new TextEncoder();
const uuid7 = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const refreshStates = ['running', 'succeeded', 'failed', 'indeterminate'];
const refreshCategories = ['authority', 'capacity', 'cancelled', 'deadline', 'network', 'format', 'limit', 'storage', 'indeterminate', 'cleanup'];

function requireValue(condition) {
    if (!condition) throw new Error('Invalid catalog response');
}

function object(value) {
    requireValue(value !== null && typeof value === 'object' && !Array.isArray(value));
    return value;
}

function text(value, max = 4096) {
    requireValue(typeof value === 'string' && value.length <= max && encoder.encode(value).length <= max);
    return value;
}

function nullableText(value, max = 4096) { return value === null ? null : text(value, max); }
function id(value) { requireValue(typeof value === 'string' && uuid7.test(value)); return value; }
function boolean(value) { requireValue(typeof value === 'boolean'); return value; }
function timestamp(value) { instant(value); return value; }
function nullableTimestamp(value) { return value === null ? null : timestamp(value); }
function list(value, max, decode) {
    requireValue(Array.isArray(value) && value.length <= max);
    return value.map(decode);
}
function variant(value) {
    object(value);
    return { language: nullableText(value.language, 64), text: text(value.text) };
}

function guide(value) {
    if (value === null) return null;
    object(value);
    return {
        undeclared: boolean(value.undeclared), ambiguous: boolean(value.ambiguous),
        declarations: list(value.declarations, 8, (entry) => ({ display_names: list(object(entry).display_names, 4, variant) })),
    };
}

function context(value, profileId, expected) {
    object(value);
    requireValue(value.profile_id === profileId);
    const revision = text(value.profile_context_revision, 19);
    requireValue(/^[1-9]\d{0,18}$/.test(revision) && BigInt(revision) <= 9223372036854775807n);
    const snapshot = id(value.snapshot_id);
    if (expected) requireValue(expected.profile_context_revision === revision && expected.snapshot_id === snapshot
        && instant(expected.imported_at).micros === instant(value.imported_at).micros);
    requireValue(Number.isInteger(value.channel_count) && value.channel_count >= 0 && value.channel_count <= 10000);
    const refresh = object(value.latest_refresh);
    requireValue(refreshStates.includes(refresh.state) && (refresh.category === null || refreshCategories.includes(refresh.category)));
    return {
        profile_id: profileId, profile_context_revision: revision, snapshot_id: snapshot,
        imported_at: timestamp(value.imported_at), observed_at: timestamp(value.observed_at), channel_count: value.channel_count,
        latest_refresh: { state: refresh.state, category: refresh.category, attempted_at: timestamp(refresh.attempted_at), finished_at: nullableTimestamp(refresh.finished_at) },
    };
}

function channel(value) {
    object(value);
    return { id: id(value.id), name: text(value.name), tvg_name: nullableText(value.tvg_name), group: nullableText(value.group), number: nullableText(value.number), guide: guide(value.guide) };
}

function programme(value) {
    object(value);
    const quality = object(value.quality);
    return {
        id: id(value.id), start_raw: nullableText(value.start_raw), stop_raw: nullableText(value.stop_raw), clump_index_raw: nullableText(value.clump_index_raw),
        starts_at: nullableTimestamp(value.starts_at), stops_at: nullableTimestamp(value.stops_at),
        titles: list(value.titles, 4, variant), subtitles: list(value.subtitles, 4, variant), descriptions: list(value.descriptions, 4, variant), categories: list(value.categories, 8, variant),
        ratings: list(value.ratings, 8, (entry) => ({ system: nullableText(object(entry).system), value: text(entry.value) })),
        quality: { uncertain: boolean(quality.uncertain), conflict: boolean(quality.conflict), overlap: boolean(quality.overlap), clump: boolean(quality.clump) },
    };
}

export function decodeCatalogPage(value, { profileId, expected = null, lane = 'channels', channelId = null, window = null }) {
    object(value);
    const result = {
        context: context(value.context, profileId, expected),
        items: list(value.items, 20, lane === 'channels' ? channel : programme),
        has_more: boolean(value.has_more), cursor: value.cursor == null ? null : text(value.cursor, 2048),
        guide: null,
    };
    requireValue(new Set(result.items.map((entry) => entry.id)).size === result.items.length);
    requireValue(result.has_more ? Boolean(result.cursor) && result.items.length > 0 : result.cursor === null);
    if (lane !== 'channels') {
        requireValue(value.channel_id === channelId && value.view === lane);
        if (lane === 'window') {
            object(value.window);
            requireValue(instant(value.window.from).micros === instant(window.from).micros && instant(value.window.to).micros === instant(window.to).micros);
        } else requireValue(value.window === null);
        result.guide = guide(value.guide);
    }
    return result;
}
