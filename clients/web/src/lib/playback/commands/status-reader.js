// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { path, valid } from './schema.js';

export function createStatusReader({ transport, owns, now, wait }) {
    let pending = null;
    let last = -Infinity;
    function read(record, token, suffix = '', cleanup = false, deadline = Infinity) {
        valid(!pending && record);
        const original = record;
        const run = async () => {
            const delay = Math.max(0, 1000 - (now() - last));
            if (delay >= deadline - now()) { await wait(Math.max(0, deadline - now())); return null; }
            if (delay) await wait(delay);
            valid(owns(token, original.start));
            const remaining = Math.floor(deadline - now());
            if (remaining < 1) return null;
            last = now();
            return transport.call('GET', path(original.start, suffix), original.scope,
                { cleanup, timeoutMs: Math.min(5000, remaining) });
        };
        const operation = run();
        pending = operation;
        return operation.finally(() => { if (pending === operation) pending = null; });
    }
    return { read, busy: () => !!pending, drain: () => pending || Promise.resolve() };
}
