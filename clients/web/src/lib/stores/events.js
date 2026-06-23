/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { writable, derived } from 'svelte/store';

const EVENTS_URL = '/api/v1/events';

function createEventsStore() {
    let eventSource = null;
    const dispatchers = new Map();
    const handlers = new Map();

    const { subscribe, update } = writable({
        readyState: 'disconnected',
        lastEventId: null,
        error: null,
    });

    function makeDispatcher(type) {
        return (event) => {
            if (event.lastEventId) {
                update((s) => ({ ...s, lastEventId: event.lastEventId }));
            }
            const typeHandlers = handlers.get(type);
            if (!typeHandlers || typeHandlers.size === 0) return;
            let payload;
            try {
                payload = JSON.parse(event.data);
            } catch {
                payload = event.data;
            }
            for (const fn of typeHandlers) {
                try {
                    fn(payload, event);
                } catch (err) {
                    console.error('[events] handler error for', type + ':', err);
                }
            }
        };
    }

    function attachAllListeners(es) {
        for (const type of handlers.keys()) {
            let dispatcher = dispatchers.get(type);
            if (!dispatcher) {
                dispatcher = makeDispatcher(type);
                dispatchers.set(type, dispatcher);
            }
            es.addEventListener(type, dispatcher);
        }
    }

    function closeSource() {
        if (eventSource !== null) {
            eventSource.close();
            eventSource.onopen = null;
            eventSource.onerror = null;
            eventSource = null;
        }
    }

    return {
        subscribe,

        connect() {
            if (typeof EventSource === 'undefined') return;
            if (eventSource !== null) return;

            update((s) => ({ ...s, readyState: 'connecting', error: null }));

            const es = new EventSource(EVENTS_URL);
            eventSource = es;

            attachAllListeners(es);

            es.onopen = () => {
                if (eventSource !== es) return;
                update((s) => ({ ...s, readyState: 'connected', error: null }));
            };

            es.onerror = () => {
                if (eventSource !== es) return;
                if (es.readyState === EventSource.CLOSED) {
                    update((s) => ({
                        ...s,
                        readyState: 'disconnected',
                        error: 'connection_failed',
                    }));
                    closeSource();
                } else {
                    update((s) => ({ ...s, readyState: 'connecting' }));
                }
            };
        },

        disconnect() {
            closeSource();
            update((s) => ({
                ...s,
                readyState: 'disconnected',
                error: null,
            }));
        },

        on(type, handler) {
            if (!handlers.has(type)) {
                handlers.set(type, new Set());
            }
            handlers.get(type).add(handler);

            if (eventSource !== null && !dispatchers.has(type)) {
                const dispatcher = makeDispatcher(type);
                dispatchers.set(type, dispatcher);
                eventSource.addEventListener(type, dispatcher);
            }

            return () => {
                const typeHandlers = handlers.get(type);
                if (!typeHandlers) return;
                typeHandlers.delete(handler);
                if (typeHandlers.size === 0) {
                    handlers.delete(type);
                }
            };
        },

        off(type, handler) {
            const typeHandlers = handlers.get(type);
            if (!typeHandlers) return;
            typeHandlers.delete(handler);
            if (typeHandlers.size === 0) {
                handlers.delete(type);
            }
        },

        getState() {
            let state;
            const unsub = subscribe((s) => {
                state = s;
            });
            unsub();
            return state;
        },
    };
}

export const events = createEventsStore();

export const connectionState = derived(events, ($events) => $events.readyState);

export const isConnected = derived(
    events,
    ($events) => $events.readyState === 'connected',
);

export const isConnecting = derived(
    events,
    ($events) => $events.readyState === 'connecting',
);

export const lastEventId = derived(events, ($events) => $events.lastEventId);
