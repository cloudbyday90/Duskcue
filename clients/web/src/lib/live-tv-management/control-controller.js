/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { createManagementScope } from './scope.js';
import { representation, managementRequest, controlPath } from './http.js';
import { control, requireValue, etag } from './schema.js';

export function createOperatorController(options) {
    const scope = createManagementScope(options);
    const reload = () => scope.read(async (context, signal) => ({ control: await representation(controlPath, context, control, signal) }));
    return { subscribe: scope.subscribe, getState: () => scope.state, reload, erase: scope.erase,
        configure(enabled) {
            const saved = scope.state.data?.control;
            return scope.write(async (context) => {
                requireValue(saved && typeof enabled === 'boolean');
                const response = await managementRequest('PUT', controlPath, { context, body: { enabled }, version: saved.etag });
                requireValue(response.status === 200); etag(response.etag); control(response.body);
            }, async () => { await reload(); scope.publish({ notice: 'saved' }); });
        }, invalidate: scope.clear, dispose: scope.dispose,
    };
}
