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

import { paraglideMiddleware } from '$lib/paraglide/server.js';
import { getTextDirection } from '$lib/paraglide/runtime.js';

const API_URL = process.env.DUSKCUE_INTERNAL_API_URL || 'http://127.0.0.1:48028';
const PROXY_PREFIXES = ['/api/', '/health', '/metrics'];
const HOP_BY_HOP_HEADERS = new Set([
    'connection',
    'keep-alive',
    'proxy-authenticate',
    'proxy-authorization',
    'te',
    'trailer',
    'transfer-encoding',
    'upgrade',
]);

const isBackendRoute = (pathname) =>
    pathname === '/api' ||
    pathname === '/metrics' ||
    pathname === '/health' ||
    PROXY_PREFIXES.some((prefix) => pathname.startsWith(prefix));

const filteredHeaders = (headers) => {
    const next = new Headers(headers);

    for (const header of HOP_BY_HOP_HEADERS) {
        next.delete(header);
    }

    return next;
};

const proxyBackend = async (event) => {
    const target = new URL(`${event.url.pathname}${event.url.search}`, API_URL);
    const headers = filteredHeaders(event.request.headers);
    headers.set('x-forwarded-host', event.url.host);
    headers.set('x-forwarded-proto', event.url.protocol.replace(':', ''));
    headers.set('x-forwarded-for', event.getClientAddress());

    const init = {
        method: event.request.method,
        headers,
        redirect: /** @type {RequestRedirect} */ ('manual'),
    };

    if (event.request.method !== 'GET' && event.request.method !== 'HEAD') {
        init.body = event.request.body;
        init.duplex = 'half';
    }

    const response = await fetch(target, init);
    return new Response(response.body, {
        status: response.status,
        statusText: response.statusText,
        headers: filteredHeaders(response.headers),
    });
};

export const handle = async ({ event, resolve }) => {
    if (isBackendRoute(event.url.pathname)) {
        return proxyBackend(event);
    }

    return paraglideMiddleware(event.request, ({ request, locale }) => {
        event.request = request;

        return resolve(event, {
            transformPageChunk: ({ html }) =>
                html.replace('%lang%', locale).replace('%dir%', getTextDirection(locale)),
        });
    });
};
