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

const API_BASE = '/api/v1';

let bearerToken = null;

export function setBearerToken(token) {
    bearerToken = token;
}

export function clearBearerToken() {
    bearerToken = null;
}

export function buildApiUrl(path, params = {}) {
    const search = new URLSearchParams();
    appendParams(search, params);
    const query = search.toString();
    return query ? `${API_BASE}${path}?${query}` : `${API_BASE}${path}`;
}

export class ApiError extends Error {
    constructor(problem) {
        super(problem.detail || problem.title || `HTTP ${problem.status}`);
        this.name = 'ApiError';
        this.type = problem.type || '';
        this.title = problem.title || '';
        this.status = problem.status || 0;
        this.detail = problem.detail || '';
        this.traceId = problem.trace_id || '';
        this.instance = problem.instance || '';
        this.errors = problem.errors || null;
        this.retryAfter = null;
    }

    get isValidation() {
        return Array.isArray(this.errors);
    }

    get isRateLimited() {
        return this.status === 429;
    }

    get isUnauthorized() {
        return this.status === 401;
    }

    get isForbidden() {
        return this.status === 403;
    }

    get isNotFound() {
        return this.status === 404;
    }

    get isConflict() {
        return this.status === 409;
    }

    get isServerError() {
        return this.status >= 500;
    }

    fieldError(fieldName) {
        if (!this.errors) return undefined;
        return this.errors.find((e) => e.field === fieldName);
    }
}

function appendParams(search, params) {
    for (const [key, value] of Object.entries(params)) {
        if (value === undefined || value === null) continue;
        if (Array.isArray(value)) {
            if (value.length === 0) continue;
            search.set(key, value.join(','));
        } else if (typeof value === 'boolean') {
            search.set(key, value ? 'true' : 'false');
        } else {
            search.set(key, String(value));
        }
    }
}

function buildHeaders(options) {
    const headers = {
        Accept: 'application/json',
    };
    if (bearerToken) {
        headers['Authorization'] = `Bearer ${bearerToken}`;
    }
    if (options.ifNoneMatch) {
        headers['If-None-Match'] = options.ifNoneMatch;
    }
    if (options.headers) {
        Object.assign(headers, options.headers);
    }
    return headers;
}

export async function request(method, path, options = {}) {
    const search = new URLSearchParams();
    if (options.params) {
        appendParams(search, options.params);
    }
    const query = search.toString();
    const url = query ? `${API_BASE}${path}?${query}` : `${API_BASE}${path}`;

    const headers = buildHeaders(options);

    const hasBody = options.body !== undefined && options.body !== null;
    const isFormData = typeof FormData !== 'undefined' && options.body instanceof FormData;
    if (hasBody && !isFormData) {
        headers['Content-Type'] = 'application/json';
    }

    const fetchOptions = {
        method,
        headers,
        credentials: 'same-origin',
    };
    if (options.signal) {
        fetchOptions.signal = options.signal;
    }
    if (hasBody) {
        fetchOptions.body = isFormData ? options.body : JSON.stringify(options.body);
    }

    let response;
    try {
        response = await fetch(url, /** @type {RequestInit} */ (fetchOptions));
    } catch (err) {
        if (err.name === 'AbortError') throw err;
        throw new ApiError({
            type: '/errors/network',
            title: 'NETWORK_ERROR',
            status: 0,
            detail: err.message || 'Network request failed',
        });
    }

    if (options.returnResponse) {
        return response;
    }

    if (response.status === 204 || response.status === 304) {
        return null;
    }

    if (!response.ok) {
        let problem;
        try {
            problem = await response.json();
        } catch {
            problem = {
                type: `/errors/http_${response.status}`,
                title: `HTTP_${response.status}`,
                status: response.status,
                detail: response.statusText || 'Unknown error',
            };
        }
        const error = new ApiError(problem);
        const retryAfter = response.headers.get('Retry-After');
        if (retryAfter) {
            error.retryAfter = parseInt(retryAfter, 10);
        }
        throw error;
    }

    const contentType = response.headers.get('Content-Type') || '';
    if (contentType.includes('application/json')) {
        return response.json();
    }

    return null;
}

export function get(path, params = {}, options = {}) {
    return request('GET', path, { ...options, params });
}

export function post(path, body = undefined, options = {}) {
    return request('POST', path, { ...options, body });
}

export function patch(path, body = undefined, options = {}) {
    return request('PATCH', path, { ...options, body });
}

export function put(path, body = undefined, options = {}) {
    return request('PUT', path, { ...options, body });
}

export function del(path, options = {}) {
    return request('DELETE', path, options);
}
