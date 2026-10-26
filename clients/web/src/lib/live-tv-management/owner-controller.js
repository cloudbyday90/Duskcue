/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { createManagementScope } from './scope.js';
import { representation, managementRequest, ownerPath } from './http.js';
import * as schema from './schema.js';

export function createOwnerPolicyController(options) {
    const scope = createManagementScope(options); let history = [null]; let position = 0;
    async function reload() {
        await scope.read(async (context, signal, session) => {
            if (!session.grant.enabled) throw Object.assign(new Error('Owner proof required'), { title: 'OWNER_001', status: 403 });
            const saved = await representation(ownerPath, context, (v) => schema.assignment(v, context.user_id, true), signal);
            const page = await managementRequest('GET', `${ownerPath}/profiles`, { context, signal, params: { limit: 20, ...(history[position] ? { after: history[position] } : {}) } });
            schema.requireValue(page.status === 200); const profiles = schema.profiles(page.body, saved.body);
            const current = await representation(ownerPath, context, (v) => schema.assignment(v, context.user_id, true), signal);
            schema.requireValue(current.etag === saved.etag);
            return { assignment: saved, profiles, position, fullHistory: history.length >= 64 };
        });
    }
    return { subscribe: scope.subscribe, getState: () => scope.state, reload, erase: scope.erase,
        async page(direction) {
            if (scope.state.busy) return;
            if (direction === 'first') { history = [null]; position = 0; }
            else if (direction === 'previous' && position > 0) position -= 1;
            else if (direction === 'next' && scope.state.data?.profiles.next_cursor && history.length < 64) { history = history.slice(0, position + 1); history.push(scope.state.data.profiles.next_cursor); position += 1; }
            else return;
            await reload();
        },
        patch(body) {
            const saved = scope.state.data?.assignment;
            return scope.write(async (context) => {
                schema.requireValue(saved);
                const result = await managementRequest('PATCH', ownerPath, { context, body, version: saved.etag });
                schema.requireValue(result.status === 200); schema.etag(result.etag); schema.assignment(result.body, context.user_id, true);
            }, async () => { await reload(); scope.publish({ notice: 'saved' }); });
        },
        invalidate: scope.clear, dispose: scope.dispose,
    };
}
