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

import { events } from '$lib/stores/events.js';

let bridgeStarted = false;
let eventUnsubscribers = [];
let sseUnsubscriber = null;

export function isTauriDesktop() {
    return typeof window !== 'undefined' && !!window.__TAURI_INTERNALS__;
}

async function tauriApi() {
    if (!isTauriDesktop()) return null;
    const [core, event] = await Promise.all([
        import('@tauri-apps/api/core'),
        import('@tauri-apps/api/event'),
    ]);
    return { invoke: core.invoke, listen: event.listen };
}

export async function startDesktopBridge(goto) {
    if (bridgeStarted || !isTauriDesktop()) return;
    const api = await tauriApi();
    if (!api) return;

    bridgeStarted = true;
    eventUnsubscribers = await Promise.all([
        api.listen('duskcue://navigate', (event) => {
            const route = event.payload?.route;
            if (isAllowedRoute(route)) {
                goto(route);
            }
        }),
        api.listen('duskcue://playback-toggle', () => {
            window.dispatchEvent(new CustomEvent('duskcue:desktop-playback-toggle'));
        }),
    ]);

    sseUnsubscriber = events.on('notification', (payload) => {
        showNativeNotification(payload);
    });
}

export function stopDesktopBridge() {
    for (const unsub of eventUnsubscribers) {
        unsub();
    }
    eventUnsubscribers = [];
    if (sseUnsubscriber) {
        sseUnsubscriber();
        sseUnsubscriber = null;
    }
    bridgeStarted = false;
}

export async function pickLibraryFolder() {
    const api = await tauriApi();
    if (!api) return null;
    return api.invoke('pick_library_folder');
}

async function showNativeNotification(payload) {
    if (!payload || (!payload.title && !payload.body)) return;
    const api = await tauriApi();
    if (!api) return;
    try {
        await api.invoke('show_native_notification', {
            req: {
                title: payload.title || 'Duskcue',
                body: payload.body || '',
                link: payload.link || null,
            },
        });
    } catch {
    }
}

function isAllowedRoute(route) {
    if (typeof route !== 'string' || !route.startsWith('/')) return false;
    if (route.includes('//') || route.includes('\\')) return false;
    return [
        /^\/dashboard$/,
        /^\/libraries(?:\/[A-Za-z0-9_.-]+)?$/,
        /^\/media\/[A-Za-z0-9_.-]+$/,
        /^\/play\/[A-Za-z0-9_.-]+$/,
        /^\/settings(?:\/[A-Za-z0-9_.-]+)?$/,
        /^\/auth\/link$/,
    ].some((pattern) => pattern.test(route));
}
