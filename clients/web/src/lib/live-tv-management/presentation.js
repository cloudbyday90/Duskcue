/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { m } from '../paraglide/messages.js';

export function problemText(kind) {
    const messages = { unlock: m.iptv_manage_locked, validation: m.iptv_manage_validation, changed: m.iptv_manage_changed, forbidden: m.iptv_manage_forbidden, missing: m.iptv_manage_missing, session: m.iptv_manage_session, uncertain: m.iptv_manage_uncertain, unavailable: m.iptv_manage_unavailable };
    return (messages[kind] || m.iptv_manage_uncertain)();
}
export function observationText(value) {
    if (!value) return '';
    if (value.observation === 'not_current') return m.iptv_manage_not_current();
    if (value.observation === 'unfinished_uncertain') return m.iptv_manage_unfinished_uncertain();
    if (!value.latest) return m.iptv_manage_never_imported();
    const states = { running: m.iptv_manage_running, succeeded: m.iptv_manage_succeeded, failed: m.iptv_manage_failed, indeterminate: m.iptv_manage_indeterminate };
    return (states[value.latest.state] || m.iptv_manage_indeterminate)();
}
