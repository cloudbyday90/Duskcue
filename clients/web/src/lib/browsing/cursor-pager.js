/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { writable } from 'svelte/store';

export function createCursorPager(fetchPage, pageSize = 24) {
    const empty = { items: [], cursor: null, hasMore: false, loadedPages: 0, loading: false, loadingMore: false, error: null, moreError: null, facets: null };
    const store = writable(empty);
    let state = empty;
    let controller = null;
    let generation = 0;
    let signature = null;
    let params = {};
    let pages = [];
    let requestedDepth = 1;

    function publish(update) {
        state = { ...state, ...update };
        store.set(state);
    }

    function visible(depth) {
        const selected = pages.slice(0, depth);
        const last = selected.at(-1);
        const items = [...new Map(selected.flatMap((entry) => entry.items).map((item) => [item.id, item])).values()];
        return { items, cursor: last?.cursor || null, hasMore: !!last?.hasMore, loadedPages: selected.length, facets: pages[0]?.facets || null };
    }

    async function load(query = {}, depth = 1) {
        const key = JSON.stringify(query);
        controller?.abort();
        controller = new AbortController();
        const signal = controller.signal;
        const run = ++generation;
        requestedDepth = Math.max(1, depth);
        if (key !== signature) {
            signature = key;
            params = { ...query };
            pages = [];
        }
        publish({ ...visible(requestedDepth), loading: pages.length === 0, loadingMore: pages.length > 0 && requestedDepth > pages.length, error: null, moreError: null });
        try {
            while (pages.length < requestedDepth && (!pages.length || pages.at(-1).hasMore)) {
                const previous = pages.at(-1);
                const response = await fetchPage({ ...params, limit: pageSize, ...(previous?.cursor ? { cursor: previous.cursor } : {}) }, { signal });
                if (run !== generation || signal.aborted) return state;
                const nextCursor = response.cursor || null;
                if (response.has_more && (!nextCursor || pages.some((entry) => entry.cursor === nextCursor))) {
                    throw new Error('The next page could not be loaded.');
                }
                pages.push({ items: response.items || [], cursor: nextCursor, hasMore: !!response.has_more, facets: response.facets || null });
                publish(visible(requestedDepth));
            }
        } catch (error) {
            if (run !== generation || signal.aborted || error.name === 'AbortError') return state;
            publish(pages.length ? { moreError: error } : { error });
        } finally {
            if (run === generation) publish({ loading: false, loadingMore: false });
        }
        return state;
    }

    return {
        subscribe: store.subscribe,
        load,
        loadAll(query = {}) {
            return load(query, Infinity);
        },
        next() {
            if (!state.hasMore || state.loading || state.loadingMore) return Promise.resolve(state);
            return load(params, state.loadedPages + 1);
        },
        retry() {
            return load(params, requestedDepth);
        },
        refresh() {
            signature = null;
            return load(params, requestedDepth);
        },
        getState() {
            return state;
        },
        dispose() {
            generation += 1;
            controller?.abort();
            controller = null;
        },
    };
}
