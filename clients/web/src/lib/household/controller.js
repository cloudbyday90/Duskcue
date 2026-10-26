/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { writable } from 'svelte/store';
import { householdRequest, getOwnerStatus, startOwnerPasskey, listOwnerCandidates } from '../api/household.js';
import { ownerStatus, valid } from './schema.js';
import { instant } from '../time/instant.js';
import { ownerCeremony } from './ceremony.js';

function failure(error) {
    if (error.title === 'OWNER_002') return 'invalid';
    if (error.title === 'OWNER_003' || error.status === 429) return 'busy';
    if (error.title === 'OWNER_007' || error.status === 409) return 'changed';
    if (error.status === 401) return 'session';
    return 'failed';
}

export function createHouseholdController({ auth, profileId, native = false, ceremony = ownerCeremony }) {
    const empty = () => ({ context: null, status: null, busy: false, error: null, passkeys: [], retryAt: 0, deadline: 0 });
    let state = empty();
    const store = writable(state);
    let revision = 0;
    let disposed = false;
    let controller = null;
    let expiry = null;
    let candidate = null;
    function publish(patch) { state = { ...state, ...patch }; if (!disposed) store.set(state); }
    function clear() { revision += 1; controller?.abort(); candidate?.abort(); candidate = null; clearTimeout(expiry); state = empty(); if (!disposed) store.set(state); }
    function current(run, lease) { return !disposed && revision === run && auth.isCurrent(lease, true) && lease.profileId === profileId; }
    async function failed(error) {
        clearTimeout(expiry);
        const seconds = [429, 503].includes(error.status) && Number.isSafeInteger(error.retryAfter) && error.retryAfter > 0 ? error.retryAfter : 0;
        publish({ status: null, context: null, passkeys: [], busy: false, error: failure(error), retryAt: Math.min(Number.MAX_SAFE_INTEGER, Date.now() + seconds * 1000), deadline: 0 });
        if (failure(error) === 'session') {
            const lease = auth.capture();
            await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: lease.origin, userId: lease.userId } });
        }
    }
    async function load() {
        if (disposed || state.busy || state.retryAt > Date.now()) return;
        clear(); const run = revision; let lease = auth.capture();
        controller = new AbortController();
        publish({ busy: true });
        const started = performance.now();
        try {
            const read = await auth.readContext(controller.signal);
            const context = read.context; lease = read.lease;
            if (!current(run, lease)) return;
            valid(context.profile_id === profileId && !context.profile_selection_required && context.transport !== 'legacy');
            const status = ownerStatus(await getOwnerStatus(context, controller.signal), context);
            if (!current(run, lease)) return;
            const remaining = status.expires_at ? Math.min(300_000, Number(instant(status.expires_at).micros - instant(context.observed_at).micros) / 1000) - (performance.now() - started) : 0;
            if (remaining <= 0) status.expires_at = null;
            publish({ context, status, busy: false, deadline: remaining > 0 ? performance.now() + remaining : 0 });
            if (remaining > 0) expiry = setTimeout(() => publish({ status: { ...state.status, expires_at: null }, deadline: 0 }), remaining);
        } catch (error) { if (current(run, lease)) await failed(error); }
    }
    async function prove(submit) {
        if (disposed || state.busy || !state.context || state.retryAt > Date.now()) return;
        const expected = state.context;
        const run = ++revision;
        publish({ busy: true, status: null, error: null, passkeys: [], deadline: 0 });
        clearTimeout(expiry);
        const timer = setTimeout(() => { if (!disposed && revision === run) publish({ error: 'pending' }); }, 15_000);
        try {
            await auth.replaceSession(submit, expected);
            if (disposed || revision !== run) return;
            publish({ busy: false }); await load();
        } catch (error) { if (!disposed && revision === run) await failed(error); }
        finally { clearTimeout(timer); }
    }
    return {
        subscribe: store.subscribe, getState: () => state, load, clear,
        password(password, handoff = '') {
            valid(new TextEncoder().encode(password).byteLength > 0 && new TextEncoder().encode(password).byteLength <= 1024 && new TextEncoder().encode(handoff).byteLength <= 128);
            const context = state.context;
            return prove(() => householdRequest('POST', '/household-management/password', { context, body: { password, ...(handoff ? { handoff } : {}) } }));
        },
        async candidates() {
            if (disposed || state.busy || !state.context) return;
            const run = revision; const context = state.context;
            candidate?.abort(); const pending = new AbortController(); candidate = pending;
            try {
                const body = await listOwnerCandidates(pending.signal);
                const read = await auth.readContext(pending.signal);
                if (candidate !== pending || !current(run, read.lease)) return;
                valid(read.context.context_id === context.context_id && Array.isArray(body?.items) && body.items.length <= 64);
                const passkeys = body.items.map((key) => { valid(typeof key.id === 'string' && typeof key.name === 'string' && [...key.name].length <= 200); return { id: key.id, name: key.name }; });
                publish({ passkeys });
            } catch (error) { if (candidate === pending && !pending.signal.aborted && !disposed && revision === run) await failed(error); }
        },
        async passkey(operation = 'unlock', handoff = '', passkeyId = '', name = '') {
            if (native || disposed || state.busy || !state.context) return;
            const context = state.context; const run = ++revision; const lease = auth.capture();
            controller = new AbortController();
            clearTimeout(expiry);
            publish({ busy: true, error: null, status: null });
            try {
                const body = operation === 'unlock' ? { operation } : operation === 'enroll' ? { operation, handoff, passkey_id: passkeyId } : { operation, handoff, name };
                const start = await startOwnerPasskey(body, context, controller.signal);
                if (!current(run, lease)) return;
                valid(typeof start?.challenge_id === 'string');
                const credential = await ceremony(start.options, operation === 'register', controller.signal);
                if (!current(run, lease)) return;
                publish({ busy: false });
                await prove(() => householdRequest('POST', '/household-management/passkey/finish', { context, body: { challenge_id: start.challenge_id, credential } }));
            } catch (error) { if (current(run, lease)) await failed(error); }
        },
        async lock() {
            if (disposed || state.busy || !state.context) return;
            const context = state.context; const run = ++revision; const lease = auth.capture();
            clearTimeout(expiry); publish({ busy: true, status: null, deadline: 0 });
            try { await householdRequest('DELETE', '/household-management/grant', { context }); if (current(run, lease)) { publish({ busy: false }); await load(); } }
            catch (error) { if (current(run, lease)) await failed(error); }
        },
        dispose() { disposed = true; clear(); },
    };
}
