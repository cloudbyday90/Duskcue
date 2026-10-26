/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { getOwnerStatus } from '../api/household.js';
import { ownerStatus } from '../household/schema.js';
import { instant } from '../time/instant.js';
import { requireValue } from './schema.js';

export async function readManagementSession(auth, profileId, signal) {
    const started = performance.now();
    const read = await auth.readContext(signal);
    requireValue(read.context.profile_id === profileId && !read.context.profile_selection_required && read.context.transport !== 'legacy');
    const status = ownerStatus(await getOwnerStatus(read.context, signal), read.context);
    const remaining = status.expires_at ? Math.min(300000, Number(instant(status.expires_at).micros - instant(read.context.observed_at).micros) / 1000) - (performance.now() - started) : 0;
    return { ...read, grant: { enabled: remaining > 0, expires_at: remaining > 0 ? status.expires_at : null, deadline: remaining > 0 ? performance.now() + remaining : 0 } };
}

export async function confirmManagementSession(auth, expected, signal) {
    const read = await auth.readContext(signal);
    requireValue(read.context.context_id === expected.context.context_id && auth.isCurrent(expected.lease, true));
    return read;
}

export function failureKind(error) {
    if (error.title === 'OWNER_001' || error.title === 'OWNER_002') return 'unlock';
    if (error.title === 'OWNER_007' || [409, 412, 428].includes(error.status)) return 'changed';
    if (error.status === 401) return 'session';
    if (error.status === 403) return 'forbidden';
    if (error.status === 404) return 'missing';
    if (['IPTV_005', 'IPTV_006', 'VALID_001', 'IPTV_REFRESH_001'].includes(error.title)) return 'validation';
    if ([429, 503].includes(error.status)) return 'unavailable';
    return 'uncertain';
}
