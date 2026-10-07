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

import { writable, derived, get } from 'svelte/store';
import { buildApiUrl } from '../api/core.js';
import { createEventTransport, EVENT_SOURCE_CLOSED } from '../events/transport.js';

export function createEventsStore({ createSource = createEventTransport, eventsUrl = () => buildApiUrl('/events') } = {}) {
    let eventSource = null;
    const dispatchers = new Map();
    const handlers = new Map();

    const { subscribe, update } = writable({
        readyState: 'disconnected',
        lastEventId: null,
        error: null,
    });

    function makeDispatcher(type, source) {
        return (event) => {
            if (eventSource !== source) return;
            if (typeof event.lastEventId === 'string') {
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
            const dispatcher = makeDispatcher(type, es);
            dispatchers.set(type, dispatcher);
            es.addEventListener(type, dispatcher);
        }
    }

    function closeSource() {
        if (eventSource !== null) {
            for (const [type, dispatcher] of dispatchers) eventSource.removeEventListener(type, dispatcher);
            eventSource.close();
            eventSource.onopen = null;
            eventSource.onerror = null;
            eventSource = null;
            dispatchers.clear();
        }
    }

    function removeHandler(type, handler) {
        const typeHandlers = handlers.get(type);
        if (!typeHandlers) return;
        typeHandlers.delete(handler);
        if (typeHandlers.size === 0) {
            handlers.delete(type);
            const dispatcher = dispatchers.get(type);
            if (dispatcher) eventSource?.removeEventListener(type, dispatcher);
            dispatchers.delete(type);
        }
    }

    return {
        subscribe,

        connect() {
            if (eventSource !== null) return;

            update((s) => ({ ...s, readyState: 'connecting', lastEventId: null, error: null }));
            let es;
            try { es = createSource({ url: eventsUrl(), isCurrent: () => eventSource === es }); }
            catch {
                update((s) => ({ ...s, readyState: 'disconnected', error: 'connection_failed' }));
                return;
            }
            eventSource = es;

            attachAllListeners(es);

            es.onopen = () => {
                if (eventSource !== es) return;
                update((s) => ({ ...s, readyState: 'connected', error: null }));
            };

            es.onerror = () => {
                if (eventSource !== es) return;
                if (es.readyState === EVENT_SOURCE_CLOSED) {
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
                lastEventId: null,
                error: null,
            }));
        },

        on(type, handler) {
            if (!handlers.has(type)) {
                handlers.set(type, new Set());
            }
            handlers.get(type).add(handler);

            if (eventSource !== null && !dispatchers.has(type)) {
                const dispatcher = makeDispatcher(type, eventSource);
                dispatchers.set(type, dispatcher);
                eventSource.addEventListener(type, dispatcher);
            }

            return () => {
                removeHandler(type, handler);
            };
        },

        off(type, handler) {
            removeHandler(type, handler);
        },

        getState() {
            return get({ subscribe });
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
