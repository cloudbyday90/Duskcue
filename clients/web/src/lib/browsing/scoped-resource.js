/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { writable } from 'svelte/store';

export function createScopedResource(fetchResource) {
    const store = writable({ item: null, loading: false, error: null });
    let generation = 0;
    let controller = null;
    let currentId = null;

    async function load(id) {
        currentId = id;
        controller?.abort();
        controller = new AbortController();
        const signal = controller.signal;
        const run = ++generation;
        store.set({ item: null, loading: true, error: null });
        try {
            const item = await fetchResource(id, { signal });
            if (run === generation && !signal.aborted) store.set({ item, loading: false, error: null });
        } catch (error) {
            if (run === generation && !signal.aborted && error.name !== 'AbortError') store.set({ item: null, loading: false, error });
        }
    }

    return {
        subscribe: store.subscribe,
        load,
        retry: () => load(currentId),
        dispose() {
            generation += 1;
            controller?.abort();
        },
    };
}
