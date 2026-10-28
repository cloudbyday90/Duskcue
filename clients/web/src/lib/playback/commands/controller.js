// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { PROTOCOL, path, valid, receipt, settlement, sameScope, resource } from './schema.js';
import { createServerClock } from './clock.js';
import { createJournal } from './journal.js';
import { createCommandTransport } from './transport.js';
import { createLease } from './lease.js';
import { createCheckpoints } from './checkpoints.js';
import { configuration, operation, recoveryEvidence } from './operations.js';
import { createStatusReader } from './status-reader.js';

export function createCommandController({ transport = createCommandTransport(), journal = createJournal(), clock = createServerClock(),
    onChange = (_state) => {}, now = () => performance.now(), wait = (ms) => new Promise((resolve) => setTimeout(resolve, ms)) } = {}) {
    let record = null;
    let state = { phase: 'idle', receipt: null, streamUrl: null, progressUncertain: false, cleanupOnly: false, error: null };
    let observation = 0;
    let observing = null;
    let changing = false;
    let closing = null;
    let restored = false;
    let authDeadline = null;
    let attemptedRecovery = null;
    let probingRecovery = false;
    let held = false;
    let starting = false;
    let retryAt = 0;
    let retryTimer = null;
    let checking = null;
    const owns = (token, id) => token === observation && record?.start === id;
    const statuses = createStatusReader({ transport, owns, now, wait });
    function publish(value) { state = { ...state, ...value }; onChange({ ...state, record }); }
    function save(value) { journal.write(value); record = value; }
    function uncertain(phase = 'uncertain', error = null, preserveLease = false) {
        if (!preserveLease || !held) { lease.stop(); held = false; }
        if (record) {
            const next = { ...record, phase: state.cleanupOnly || closing ? 'cleanup' : 'uncertain' };
            try { save(next); } catch { record = next; }
        }
        publish({ phase, error, streamUrl: null });
        if ([429, 503].includes(error?.status) && Number.isFinite(error.retryAfter) && error.retryAfter > 0) {
            retryAt = now() + Math.min(86400, error.retryAfter) * 1000;
            if (retryTimer !== null) clearTimeout(retryTimer);
            publish({ retryAfter: Math.ceil((retryAt - now()) / 1000) });
            retryTimer = setTimeout(() => { retryTimer = null; publish({ retryAfter: 0 }); }, Math.max(0, retryAt - now()));
        }
    }
    function call(method, url, options = {}) { valid(record); return transport.call(method, url, record.scope, options); }
    async function context(cleanup = false) {
        const result = await transport.context();
        if (record && !sameScope(result.scope, record.scope, cleanup)) throw new Error('VOD_SCOPE_CHANGED');
        clock.observe(result.context.observed_at, result.began);
        authDeadline = result.context.session_expires_at;
        return result;
    }
    const checkpoints = createCheckpoints({
        current: (output) => !!record && output === record.output && !state.cleanupOnly && transport.current(record.scope),
        send: async (body) => {
            const id = record.start;
            const result = await call('POST', path(id, '/checkpoint'), { body });
            valid(result.body?.request_id === id);
            return result.body;
        },
        read: async () => {
            const id = record.start;
            const result = await call('GET', path(id, '/checkpoint'));
            valid(result.body?.request_id === id);
            return result.body;
        },
        onUncertain: () => publish({ progressUncertain: true }), now,
    });
    const lease = createLease({ remaining: () => held && state.receipt?.lease_deadline && authDeadline
        ? Math.min(clock.remaining(state.receipt.lease_deadline), clock.remaining(authDeadline)) : -1,
        renew: async () => {
            const original = record;
            const token = observation;
            const result = await call('POST', path(original.start, '/renew'), { body: {} });
            if (!owns(token, original.start)) return;
            const value = result.body;
            valid(value?.protocol === PROTOCOL && value.request_id === record.start && value.status === 'renewed');
            resource(value.status_url, record.scope.origin, path(record.start));
            clock.observe(value.observed_at, result.began);
            valid(clock.remaining(value.lease_deadline) > 0);
            state = { ...state, receipt: { ...state.receipt, observed_at: value.observed_at, lease_deadline: value.lease_deadline } };
        }, onUncertain: (phase) => uncertain(phase), now,
    });
    async function reconcileProgress() {
        const value = await checkpoints.reconcile();
        if (value) publish({ progressUncertain: false });
    }
    function accept(result, token, attach = true) {
        if (token !== observation || closing || !transport.current(record.scope) || restored || state.cleanupOnly) return false;
        const value = receipt(result.body, record.start, record.scope.origin);
        clock.observe(value.observed_at, result.began);
        const previouslyHeld = held;
        held = value.lease_deadline !== null && clock.remaining(value.lease_deadline) > 0 && clock.remaining(authDeadline) > 0;
        if (value.lease_deadline !== null && !held) { uncertain('expired'); return false; }
        if (held && !previouslyHeld) lease.start(); else if (!held) lease.stop();
        if (value.status === 'ready' && attach) {
            if (record.output !== value.ready.output_revision) checkpoints.reset();
            save({ ...record, output: value.ready.output_revision, phase: 'active' });
            publish({ phase: 'ready', receipt: value, streamUrl: new URL(value.stream_url, record.scope.origin).href, error: null });
            return true;
        }
        publish({ phase: value.status, receipt: value, streamUrl: null });
        return false;
    }
    async function monitor(kind, token) {
        if (observing) throw new Error('VOD_STATUS_BUSY');
        const id = record.start;
        const deadline = now() + 60_000;
        const run = async () => {
            while (now() < deadline && token === observation) {
                const began = now();
                if (kind === 'settlement') {
                    const result = await statuses.read(record, token, '/settlement', true, deadline);
                    if (!result) break;
                    if (!owns(token, id)) return false;
                    const value = settlement(result.body, record.start, record.scope.origin);
                    if (value.status === 'settled') {
                        lease.stop(); held = false; journal.clear(); record = null;
                        publish({ phase: 'released', streamUrl: null, receipt: null });
                        return true;
                    }
                    publish({ phase: value.status === 'uncertain' ? 'uncertain' : 'withdrawing', streamUrl: null });
                } else {
                    if (record.operation) {
                        const response = await statuses.read(record, token, `/operations/${record.operation}`, false, deadline);
                        if (!response) break;
                        if (!owns(token, id)) return false;
                        const value = operation(response, record);
                        if (['failed', 'cancelled'].includes(value.status)) {
                            save({ ...record, operation: null, phase: 'active' });
                            publish({ phase: 'unavailable', streamUrl: null });
                            return false;
                        }
                        if (value.status !== 'completed') { await wait(Math.max(0, 1000 - (now() - began))); continue; }
                        save({ ...record, operation: null });
                        continue;
                    }
                    const result = await statuses.read(record, token, '', false, deadline);
                    if (!result) break;
                    if (!owns(token, id)) return false;
                    if (accept(result, token)) { await reconcileProgress(); return true; }
                    if (['unavailable', 'failed', 'released', 'uncertain', 'withdrawn'].includes(state.phase)) return false;
                }
                await wait(Math.max(0, 1000 - (now() - began)));
            }
            if (token === observation) publish({ phase: 'status_required', streamUrl: null });
            return false;
        };
        observing = run();
        try { return await observing; } finally { observing = null; }
    }
    async function start(selection) {
        valid(!record && !starting && !changing && !closing);
        starting = true;
        const token = ++observation;
        try {
            await Promise.all([lease.drain(), checkpoints.drain().catch(() => {})]);
            valid(token === observation && !record);
            checkpoints.reset();
            restored = false;
            state = { ...state, cleanupOnly: false, progressUncertain: false };
            const captured = await context();
            valid(token === observation && !record);
            save({ version: 1, scope: captured.scope, start: clock.id(), operation: null, output: null, phase: 'active' });
            publish({ phase: 'preparing', error: null, streamUrl: null });
            try {
                const result = await call('POST', '/api/v1/playback-commands', { body: { protocol: PROTOCOL, request_id: record.start, selection } });
                if (token !== observation) return null;
                valid(result.status === 202);
                resource(result.location, record.scope.origin, path(record.start));
                if (!accept(result, token)) await monitor('viewer', token);
                valid(state.phase === 'ready');
                return state.receipt;
            } catch (error) { if (token === observation) uncertain('uncertain', error, held); throw error; }
        } finally { starting = false; }
    }
    async function change(action) {
        valid(record && held && ['ready', 'unavailable'].includes(state.phase) && now() >= retryAt && !record.operation && !changing && !closing && !restored && !state.cleanupOnly);
        changing = true;
        const token = observation;
        const id = record.start;
        try {
            if (observing) await observing;
            valid(owns(token, id) && !closing);
            if (checkpoints.needsReconcile()) await reconcileProgress();
            await checkpoints.flush(true);
            valid(owns(token, id) && !closing);
            await context();
            valid(owns(token, id) && !closing);
            const config = await configuration(call, record);
            valid(owns(token, id) && !closing);
            if (state.phase === 'unavailable' && config.value.state === 'unavailable' && config.value.output_revision !== record.output) {
                save({ ...record, output: config.value.output_revision });
                checkpoints.reset();
            }
            valid(config.value.output_revision === record.output && config.value.pending_operation === null);
            save({ ...record, operation: clock.id() });
            const result = await call('POST', path(record.start, '/configuration'), { etag: config.etag,
                body: { protocol: PROTOCOL, operation_id: record.operation, expected_output: record.output, action } });
            operation(result, record);
            if (token !== observation || closing) return null;
            checkpoints.discard();
            publish({ phase: action.kind === 'recover' ? 'recovering' : 'changing', streamUrl: null });
            await monitor('viewer', token);
            return state.receipt;
        } catch (error) {
            if (token === observation && !closing) {
                if ([400, 413, 415, 422].includes(error?.status)) { save({ ...record, operation: null }); publish({ error }); }
                else uncertain('uncertain', error, held);
            }
            throw error;
        } finally { changing = false; }
    }
    async function recover(failure) {
        if (!record || !held || changing || closing || probingRecovery || record.operation || attemptedRecovery === record.output) return false;
        probingRecovery = true;
        try {
            const output = record.output;
            if (checkpoints.needsReconcile()) await reconcileProgress();
            await checkpoints.flush(true);
            const confirmed = checkpoints.confirmed();
            if (!confirmed || confirmed.output_revision !== output || confirmed.sequence < 1) return false;
            const config = await configuration(call, record);
            const evidence = await recoveryEvidence(config.value, { ...failure, output });
            if (!evidence || record.output !== output || closing) return false;
            attemptedRecovery = output;
            await change({ kind: 'recover', checkpoint_sequence: confirmed.sequence, evidence });
            return true;
        } finally { probingRecovery = false; }
    }
    async function close() {
        if (closing) return closing;
        const token = ++observation;
        lease.stop(); held = false;
        if (!record) return true;
        const original = record;
        save({ ...record, phase: 'cleanup' });
        publish({ phase: 'withdrawing', streamUrl: null });
        const run = async () => {
            let timer;
            try { await Promise.race([checkpoints.flush(true), new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('VOD_PROGRESS_UNCERTAIN')), 5000); })]); }
            catch { if (owns(token, original.start)) publish({ progressUncertain: true }); }
            finally { clearTimeout(timer); checkpoints.discard(); }
            valid(record?.start === original.start);
            valid(transport.current(original.scope, true));
            valid(record?.start === original.start);
            const result = await transport.call('POST', path(original.start, '/withdraw'), original.scope, { cleanup: true, body: { protocol: PROTOCOL } });
            if (!owns(token, original.start)) return false;
            valid(result.status === 202);
            settlement(result.body, record.start, record.scope.origin);
            resource(result.location, record.scope.origin, path(record.start, '/settlement'));
            if (observing) await observing.catch(() => {});
            const released = await monitor('settlement', token);
            if (!released) throw new Error('VOD_CLEANUP_UNCONFIRMED');
            return true;
        };
        closing = Promise.resolve().then(run).catch((error) => { if (owns(token, original.start)) uncertain('uncertain', error); throw error; }).finally(() => { closing = null; });
        return closing;
    }
    async function restore() {
        const saved = journal.read();
        if (!saved) return false;
        const token = ++observation;
        record = saved; restored = true;
        const actual = await transport.context();
        if (!owns(token, saved.start)) return false;
        if (!sameScope(actual.scope, saved.scope, true)) { journal.clear(); record = null; restored = false; return false; }
        save({ ...saved, phase: 'cleanup' });
        publish({ phase: 'status_required', cleanupOnly: true, streamUrl: null });
        return true;
    }
    async function checkOwned() {
        if (!record) return false;
        if (closing) return closing;
        if (observing) return observing;
        valid(now() >= retryAt);
        lease.stop(); held = false;
        const token = ++observation;
        const id = record.start;
        try {
            await statuses.drain().catch(() => {});
            if (!owns(token, id)) return false;
            const cleanup = state.cleanupOnly || record.phase === 'cleanup';
            if (cleanup) valid(transport.current(record.scope, true)); else await context();
            if (closing) return closing;
            if (!owns(token, id)) return false;
            if (!cleanup) {
                const result = await statuses.read(record, token);
                if (!owns(token, id)) return false;
                if (accept(result, token, !record.operation) && !record.operation) { await reconcileProgress(); return true; }
            }
            return await monitor(state.cleanupOnly || record.phase === 'cleanup' ? 'settlement' : 'viewer', token);
        }
        catch (error) { if (owns(token, id)) uncertain('uncertain', error, held); throw error; }
    }
    function check() {
        if (checking) return checking;
        checking = checkOwned().finally(() => { checking = null; });
        return checking;
    }
    async function describe() {
        valid(record && held && state.phase === 'ready' && !record.operation && !state.cleanupOnly && !closing);
        const token = observation;
        const original = record;
        try {
            const config = await configuration(call, original);
            valid(owns(token, original.start) && config.value.output_revision === original.output);
            return { ...state.receipt.ready, configuration: config.value.configuration };
        } catch (error) { if (owns(token, original.start)) uncertain('uncertain', error, held); throw error; }
    }
    function invalidate(next = null) {
        ++observation; lease.stop(); held = false; checkpoints.reset(); clock.invalidate();
        if (retryTimer !== null) clearTimeout(retryTimer); retryTimer = null; retryAt = 0;
        if (!record) return;
        if (next && sameScope(next, record.scope, true)) {
            const cleanup = { ...record, phase: 'cleanup' };
            try { save(cleanup); } catch { record = cleanup; }
            publish({ phase: 'status_required', streamUrl: null, cleanupOnly: true });
        } else {
            record = null;
            publish({ phase: 'idle', streamUrl: null, receipt: null });
            try { journal.clear(); } catch {}
        }
    }
    function suspend(restoration = false) {
        if (!record) return;
        ++observation;
        if (restoration) { restored = true; publish({ cleanupOnly: true }); }
        uncertain('status_required'); clock.invalidate();
    }
    function resume() {
        if (!record) return;
        try {
            if (held && state.receipt?.lease_deadline && authDeadline && transport.current(record.scope)
                && clock.remaining(state.receipt.lease_deadline) > 0 && clock.remaining(authDeadline) > 0) return;
        } catch {}
        suspend();
    }
    async function heartbeat() {
        if (!record || !held || closing || restored || state.cleanupOnly || changing || observing || statuses.busy() || !['ready', 'unavailable'].includes(state.phase)) return;
        if (checkpoints.needsReconcile()) await reconcileProgress(); else await checkpoints.flush();
        const token = observation;
        const result = await statuses.read(record, token);
        if (token !== observation) return;
        accept(result, token);
        if (state.phase === 'unavailable') await recover({ kind: 'worker' });
    }
    return { start, change, recover, close, check, describe, restore, invalidate, suspend, resume,
        observe: checkpoints.observe, flushProgress: () => checkpoints.flush(true), checkpoint: heartbeat, getState: () => ({ ...state, record }),
        dispose: () => { ++observation; lease.stop(); checkpoints.discard(); if (retryTimer !== null) clearTimeout(retryTimer); retryTimer = null; }, active: () => !!record };
}
