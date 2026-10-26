/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { createManagementScope } from './scope.js';
import { createRefreshMonitor } from './monitor.js';
import { representation, managementRequest, assignmentPath, refreshPath } from './http.js';
import * as schema from './schema.js';
import { confirmManagementSession } from './session.js';

export function createAdministratorController(options) {
    const scope = createManagementScope(options); let target = null; let page = 1; let attempt = null; let statusController = null;
    let loadingStatus = null;
    async function status() {
        if (loadingStatus || scope.state.busy || !target || !scope.state.session || scope.disposed || scope.state.retryAt > Date.now()) return loadingStatus;
        const household = target; const session = scope.state.session;
        statusController?.abort(); const controller = new AbortController(); statusController = controller; const expected = attempt;
        const work = (async () => {
            try {
                const response = await managementRequest('GET', `${refreshPath(household)}/status`, { context: session.context, params: expected ? { attempt_id: expected } : {}, signal: controller.signal });
                schema.requireValue(response.status === 200); const value = schema.status(response.body, expected);
                await confirmManagementSession(options.auth, session, controller.signal);
                if (household !== target || !scope.current(session)) return null;
                scope.publish({ observation: value, error: null }); return value;
            } catch (error) { if (!controller.signal.aborted && household === target && scope.current(session)) { monitor.stop(); await scope.fail(error); } return null; }
        })();
        loadingStatus = work;
        try { return await work; } finally { if (loadingStatus === work) loadingStatus = null; }
    }
    const monitor = createRefreshMonitor({ read: status });
    function stop() { monitor.stop(); statusController?.abort(); loadingStatus = null; }
    async function reload() {
        stop();
        await scope.read(async (context, signal) => {
            const response = await managementRequest('GET', '/users', { context, signal, params: { page, page_size: 20, status: 'active' } });
            schema.requireValue(response.status === 200); const people = schema.users(response.body, page);
            if (!target) return { people, assignment: null, refresh: null, target: null };
            let saved;
            try { saved = await representation(assignmentPath(target), context, (v) => schema.assignment(v, target), signal); }
            catch (error) { if (error.title === 'IPTV_002' && error.status === 404) return { people, assignment: null, refresh: null, target }; throw error; }
            const refresh = await representation(refreshPath(target), context, (v) => schema.resource(v, saved.body), signal);
            return { people, assignment: saved, refresh, target };
        });
        if (target && scope.state.data?.assignment) await status();
    }
    function mutate(method, body, create = false) {
        stop(); const saved = scope.state.data?.assignment; const household = target;
        return scope.write(async (context) => {
            schema.requireValue(household && (create || saved));
            const result = await managementRequest(method, assignmentPath(household), { context, body, create, version: create ? null : saved.etag });
            schema.requireValue(result.status === (create ? 201 : method === 'DELETE' ? 204 : 200));
            if (method !== 'DELETE') { schema.etag(result.etag); schema.assignment(result.body, household); }
            return result;
        }, async () => { attempt = null; await reload(); scope.publish({ notice: 'saved' }); });
    }
    return {
        subscribe: scope.subscribe, getState: () => scope.state, reload, erase: scope.erase,
        async select(household) { if (scope.state.busy || options.commands.pending) return; target = household === null ? null : schema.id(household); attempt = null; await reload(); },
        async usersPage(number) { if (scope.state.busy) return; page = schema.integer(number, 1); target = null; attempt = null; await reload(); },
        create: (body) => mutate('PUT', body, true), patch: (body) => mutate('PATCH', body), remove: () => mutate('DELETE'),
        async start(playlist, guide) {
            stop(); const saved = scope.state.data?.refresh; const household = target;
            schema.requireValue(saved && household); schema.integer(playlist, 0, saved.body.playlist_address_count - 1); schema.integer(guide, 0, saved.body.guide_address_count - 1);
            return scope.write(async (context) => {
                attempt = null;
                const result = await managementRequest('POST', refreshPath(household), { context, body: { playlist_pin: playlist, guide_pin: guide }, version: saved.etag });
                schema.requireValue(result.status === 202); return schema.accepted(result.body, result.location, household);
            }, async (accepted) => {
                attempt = accepted.attempt_id;
                await reload();
                if (scope.state.data) { scope.publish({ notice: 'accepted' }); await monitor.start(); }
            });
        },
        checkStatus: status, stopWatching: stop,
        invalidate() { stop(); scope.clear(); scope.publish({ observation: null }); },
        dispose() { stop(); scope.dispose(); },
    };
}
