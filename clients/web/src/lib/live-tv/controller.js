/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { writable } from 'svelte/store';
import { fetchCatalogPage } from '../api/live-tv.js';
import { decodeCatalogPage } from './decode.js';
import { sixHourWindow, moveWindow } from './instant.js';
import { createCatalogPaging } from './paging.js';

function emptyState() {
    return { context: null, channels: [], programmes: [], selected: null, guide: null, window: null, view: 'window',
        channelsLoading: false, guideLoading: false, error: null,
        channelPage: { index: 0, canNext: false, atLimit: false }, guidePage: { index: 0, canNext: false, atLimit: false } };
}

function failure(error, now) {
    const status = Number(error?.status) || 0;
    const retry = [429, 503].includes(status) && Number.isSafeInteger(error.retryAfter) && error.retryAfter >= 0 ? error.retryAfter : 0;
    return { kind: status === 409 ? 'changed' : status === 401 ? 'session' : [403, 404].includes(status) ? 'unavailable' : [429, 503].includes(status) ? 'busy' : 'failed',
        notBefore: Math.min(Number.MAX_SAFE_INTEGER, now + retry * 1000) };
}

export function createCatalogController({ profileId, fetchPage = fetchCatalogPage, revalidate = async () => {}, now = Date.now }) {
    let state = emptyState();
    const store = writable(state);
    let disposed = false;
    let epoch = 0;
    let channels = createCatalogPaging();
    let guide = createCatalogPaging();
    const operations = { channels: null, guide: null };
    function publish(patch) { state = { ...state, ...patch }; store.set(state); }
    function cancel(lane) { operations[lane]?.abort(); operations[lane] = null; }
    function reset() {
        epoch += 1;
        cancel('channels'); cancel('guide');
        channels = createCatalogPaging(); guide = createCatalogPaging();
        state = emptyState(); store.set(state);
    }
    function pageState(pager) { return { index: pager.index, canNext: pager.canNext, atLimit: pager.atLimit }; }
    async function load(lane, action) {
        if (disposed || state.error) return;
        const channelLane = lane === 'channels';
        if (!channelLane && (!state.selected || state.channelsLoading)) return;
        const pager = channelLane ? channels : guide;
        if (action === 'next' && !pager.canNext) return;
        const key = channelLane ? 'channels' : 'guide';
        cancel(key);
        if (channelLane) { cancel('guide'); guide = createCatalogPaging(); }
        const cursor = pager.cursor(action);
        const controller = new AbortController();
        operations[key] = controller;
        const run = epoch;
        const current = () => !disposed && epoch === run && operations[key] === controller;
        const expected = state.context;
        const channelId = state.selected?.id || null;
        const window = state.window;
        const view = channelLane ? 'channels' : state.view;
        const query = channelLane ? { cursor } : { cursor, channelId, snapshot_id: expected.snapshot_id, ...(view === 'window' ? window : {}) };
        publish(channelLane ? { channels: [], selected: null, programmes: [], guide: null, channelsLoading: true, guideLoading: false, guidePage: pageState(guide) }
            : { programmes: [], guide: null, guideLoading: true });
        const timer = setTimeout(() => controller.abort(), 10_000);
        let onAbort;
        try {
            const aborted = new Promise((_resolve, reject) => { onAbort = () => reject(new DOMException('Catalog request ended', 'AbortError')); controller.signal.addEventListener('abort', onAbort, { once: true }); });
            const body = await Promise.race([fetchPage(view, query, controller.signal), aborted]);
            if (!current() || controller.signal.aborted) return;
            const page = decodeCatalogPage(body, { profileId, expected, lane: view, channelId, window });
            pager.accept(page.cursor);
            publish(channelLane ? { context: page.context, channels: page.items, channelsLoading: false, channelPage: pageState(pager) }
                : { context: page.context, programmes: page.items, guide: page.guide, guideLoading: false, guidePage: pageState(pager) });
        } catch (error) {
            if (!current()) return;
            const safeError = failure(error, now());
            reset();
            publish({ error: safeError });
            if (safeError.kind === 'session') {
                try { await revalidate(); } catch {}
            }
        } finally {
            clearTimeout(timer);
            controller.signal.removeEventListener('abort', onAbort);
            if (operations[key] === controller) operations[key] = null;
        }
    }
    return {
        subscribe: store.subscribe,
        getState: () => state,
        async reload() {
            if (disposed || state.error?.notBefore > now()) return;
            reset(); await load('channels', 'first');
        },
        channels: (action) => state.channelsLoading ? Promise.resolve() : load('channels', action),
        programmes: (action) => state.guideLoading ? Promise.resolve() : load('guide', action),
        async select(id) {
            const selected = state.channels.find((entry) => entry.id === id);
            if (disposed || state.error || !selected) return;
            cancel('guide'); guide = createCatalogPaging();
            publish({ selected, window: state.window || sixHourWindow(now()), guidePage: pageState(guide) });
            await load('guide', 'first');
        },
        async setView(view) {
            if (disposed || state.error || !state.selected || !['window', 'unpositionable'].includes(view)) return;
            cancel('guide'); guide = createCatalogPaging();
            publish({ view }); await load('guide', 'first');
        },
        async shiftWindow(direction) {
            if (disposed || state.error || !state.selected) return;
            cancel('guide'); guide = createCatalogPaging();
            try { publish({ window: direction === 0 ? sixHourWindow(now()) : moveWindow(state.window, direction), view: 'window' }); }
            catch { reset(); publish({ error: failure(null, now()) }); return; }
            await load('guide', 'first');
        },
        clear: reset,
        dispose() { disposed = true; reset(); },
    };
}
