/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { writable } from 'svelte/store';

export function createCommandOwner(auth) {
    let pending = null;
    const state = writable(false);
    return {
        subscribe: state.subscribe,
        get pending() { return pending; },
        run(lease, submit) {
            if (pending || auth.credentialWork() || !auth.isCurrent(lease, true)) return Promise.reject(new Error('Management command unavailable'));
            const work = Promise.resolve().then(() => { if (!auth.isCurrent(lease, true)) throw new DOMException('Context changed', 'AbortError'); return submit(); });
            pending = work; state.set(true);
            const release = () => { if (pending === work) { pending = null; state.set(false); } };
            work.then(release, release);
            return work;
        },
    };
}
