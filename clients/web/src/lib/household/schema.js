/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { instant } from '../time/instant.js';

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const revision = /^(0|[1-9]\d{0,18})$/;
export function valid(value) { if (!value) throw new Error('Invalid management response'); }

export function sessionContext(value) {
    valid(value && typeof value === 'object');
    for (const key of ['user_id', 'session_id', 'profile_id']) valid(typeof value[key] === 'string' && uuid.test(value[key]));
    for (const key of ['credential_revision', 'profile_context_revision']) valid(typeof value[key] === 'string' && revision.test(value[key]) && BigInt(value[key]) <= 9223372036854775807n);
    valid(typeof value.profile_selection_required === 'boolean' && ['cookie', 'bearer', 'legacy'].includes(value.transport));
    valid(typeof value.context_id === 'string' && /^sc1\.[A-Za-z0-9_-]{43}$/.test(value.context_id));
    instant(value.session_expires_at); instant(value.observed_at);
    return Object.fromEntries(['user_id', 'session_id', 'profile_id', 'credential_revision', 'profile_context_revision', 'profile_selection_required', 'transport', 'session_expires_at', 'observed_at', 'context_id'].map((key) => [key, value[key]]));
}

export function sameSession(first, second) {
    return Boolean(first && second && first.user_id === second.user_id && first.session_id === second.session_id);
}

export function ownerStatus(value, expected) {
    const context = sessionContext(value?.context);
    valid(context.context_id === expected.context_id && context.transport === expected.transport);
    valid(typeof value.protection_enabled === 'boolean' && typeof value.password_available === 'boolean');
    valid(Number.isSafeInteger(value.trusted_passkeys) && value.trusted_passkeys >= 0 && value.trusted_passkeys <= 64);
    if (value.expires_at !== null) instant(value.expires_at);
    return { context, protection_enabled: value.protection_enabled, password_available: value.password_available, trusted_passkeys: value.trusted_passkeys, expires_at: value.expires_at };
}

export function ownerProof(value, expected) {
    const context = sessionContext(value?.context);
    valid(sameSession(context, expected) && context.profile_id === expected.profile_id && context.credential_revision !== expected.credential_revision && context.transport === expected.transport);
    instant(value.expires_at);
    valid(instant(value.expires_at).micros <= instant(context.session_expires_at).micros);
    const bearer = expected.transport === 'bearer';
    valid(bearer ? typeof value.session_token === 'string' && value.session_token.length >= 32 && value.session_token.length <= 256 : !Object.hasOwn(value, 'session_token'));
    return { context, expires_at: value.expires_at, session_token: bearer ? value.session_token : null };
}
