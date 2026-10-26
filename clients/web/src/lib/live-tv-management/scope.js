/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { writable } from 'svelte/store';
import { readManagementSession, confirmManagementSession, failureKind } from './session.js';

export function createManagementScope({ auth, commands, profileId }) {
    let state = { data: null, session: null, busy: false, blocked: true, error: null, notice: null, retryAt: 0, secretEpoch: 0, resetEpoch: 0 };
    const store = writable(state); let disposed = false; let epoch = 0; let reading = null; let expiry = null;
    function publish(patch) { state = { ...state, ...patch }; if (!disposed) store.set(state); }
    function current(run, session) { return !disposed && run === epoch && session?.scopeEpoch === run && auth.isCurrent(session.lease, true) && session.context.profile_id === profileId; }
    function clear() { epoch += 1; reading?.abort(); clearTimeout(expiry); publish({ data: null, session: null, observation: null, busy: false, blocked: true, error: null, notice: null, secretEpoch: state.secretEpoch + 1, resetEpoch: state.resetEpoch + 1 }); }
    function armExpiry(session, run) {
        clearTimeout(expiry);
        if (session.grant.enabled) expiry = setTimeout(() => {
            if (current(run, session)) publish({ session: { ...session, grant: { enabled: false, expires_at: null, deadline: 0 } }, secretEpoch: state.secretEpoch + 1 });
        }, Math.max(0, session.grant.deadline - performance.now()));
    }
    async function failed(error, session) {
        const kind = failureKind(error); const valid = kind === 'validation';
        const seconds = [429, 503].includes(error.status) && Number.isSafeInteger(error.retryAfter) && error.retryAfter > 0 ? Math.min(error.retryAfter, 2147483) : 0;
        publish({ busy: false, error: kind, notice: null, retryAt: seconds ? Date.now() + seconds * 1000 : 0, secretEpoch: state.secretEpoch + 1,
            ...(valid ? {} : { data: null, session: null, blocked: true, resetEpoch: state.resetEpoch + 1 }) });
        if (kind === 'session' && session && auth.isCurrent(session.lease, true)) await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: session.lease.origin, userId: session.lease.userId } });
    }
    async function read(work) {
        if (disposed || state.busy || state.retryAt > Date.now()) return;
        clear(); const run = epoch; const initialLease = auth.capture(); reading = new AbortController(); let session = null;
        publish({ busy: true });
        try {
            session = { ...await readManagementSession(auth, profileId, reading.signal), scopeEpoch: run };
            if (!current(run, session)) return;
            const data = await work(session.context, reading.signal, session);
            await confirmManagementSession(auth, session, reading.signal);
            if (!current(run, session)) return;
            publish({ data, session, busy: false, blocked: false });
            armExpiry(session, run);
        } catch (error) { if (!disposed && run === epoch && auth.isCurrent(session?.lease || initialLease, true)) await failed(error, session || { lease: initialLease }); }
    }
    async function write(submit, after = null) {
        let session = state.session;
        if (disposed || state.busy || state.blocked || state.retryAt > Date.now() || commands.pending || !session || !session.grant.enabled || performance.now() >= session.grant.deadline || !current(epoch, session)) {
            publish({ error: 'unlock' }); return;
        }
        const run = ++epoch; session = { ...session, scopeEpoch: run };
        reading?.abort(); armExpiry(session, run); publish({ session, busy: true, error: null, notice: null, secretEpoch: state.secretEpoch + 1 });
        try {
            const result = await commands.run(session.lease, async () => {
                const response = await submit(session.context);
                await confirmManagementSession(auth, session);
                return response;
            });
            if (!current(run, session)) return;
            publish({ busy: false, blocked: true, notice: 'saved' });
            if (after) await after(result);
        } catch (error) { if (current(run, session)) await failed(error, session); }
    }
    return { subscribe: store.subscribe, get state() { return state; }, get disposed() { return disposed; }, current: (session) => current(epoch, session), publish, clear, read, write,
        fail: (error) => failed(error, state.session), erase: () => publish({ secretEpoch: state.secretEpoch + 1, resetEpoch: state.resetEpoch + 1 }),
        dispose() { disposed = true; clear(); } };
}
