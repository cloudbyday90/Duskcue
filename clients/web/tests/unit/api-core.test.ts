import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
    ApiError,
    buildApiUrl,
    buildRootUrl,
    clearBearerToken,
    clearServerOrigin,
    get,
    invalidateProfileScopedRequests,
    post,
    setBearerToken,
    setServerOrigin,
} from '../../src/lib/api/core.js';

function deferred<T = unknown>() {
    let resolve!: (value: T | PromiseLike<T>) => void;
    const promise = new Promise<T>((settle) => { resolve = settle; });
    return { promise, resolve };
}

function jsonResponse(body: unknown, options: ResponseInit = {}) {
    const headers = new Headers(options.headers);
    headers.set('Content-Type', 'application/json');
    return new Response(JSON.stringify(body), {
        ...options,
        headers,
    });
}

beforeEach(() => {
    clearBearerToken();
    clearServerOrigin();
    invalidateProfileScopedRequests();
});

afterEach(() => {
    clearBearerToken();
    clearServerOrigin();
    invalidateProfileScopedRequests();
});

describe('API URLs and authentication', () => {
    it('keeps web requests relative and encodes supported query values', () => {
        expect(buildApiUrl('/media-items', {
            type: ['movie', 'series'],
            search: 'space & sea',
            favorite: false,
            empty: [],
            missing: null,
        })).toBe('/api/v1/media-items?type=movie%2Cseries&search=space+%26+sea&favorite=false');
        expect(buildRootUrl('/health')).toBe('/health');
    });

    it('uses the selected desktop server origin without carrying its path', async () => {
        setServerOrigin('https://fixture-server.invalid/nested/path');
        setBearerToken('fixture-token');
        const fetchMock = vi.fn().mockResolvedValue(jsonResponse({ id: 'fixture' }));
        vi.stubGlobal('fetch', fetchMock);

        await post('/profiles/fixture/switch', { remember_on_device: false }, { profileScoped: false });

        expect(fetchMock).toHaveBeenCalledWith('https://fixture-server.invalid/api/v1/profiles/fixture/switch', expect.objectContaining({
            method: 'POST',
            credentials: 'same-origin',
            headers: expect.objectContaining({
                Authorization: 'Bearer fixture-token',
                'Content-Type': 'application/json',
            }),
            body: JSON.stringify({ remember_on_device: false }),
        }));
        expect(buildRootUrl('/health')).toBe('https://fixture-server.invalid/health');
    });
});

describe('profile request isolation', () => {
    it('aborts a pending profile-scoped fetch when the profile changes', async () => {
        let signal: AbortSignal | undefined;
        vi.stubGlobal('fetch', vi.fn((_url, options) => new Promise((_resolve, reject) => {
            signal = options.signal;
            signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true });
        })));

        const request = get('/media-items');
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });
        invalidateProfileScopedRequests();

        await rejection;
        expect(signal?.aborted).toBe(true);
    });

    it('rejects a stale fetch result even when its transport ignores cancellation', async () => {
        const response = deferred();
        vi.stubGlobal('fetch', vi.fn(() => response.promise));
        const request = get('/items/fixture/watch-data');
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });

        invalidateProfileScopedRequests();
        response.resolve(jsonResponse({ resume_position_ms: 1000 }));

        await rejection;
    });

    it('rejects body parsing that completes after the scope changes', async () => {
        const body = deferred();
        const started = deferred<void>();
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
            ok: true,
            status: 200,
            headers: new Headers({ 'Content-Type': 'application/json' }),
            json: () => {
                started.resolve();
                return body.promise;
            },
        }));
        const request = get('/media-items');
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });
        await started.promise;

        invalidateProfileScopedRequests();
        body.resolve({ items: [] });

        await rejection;
    });

    it('allows profile-selection requests to complete across scope invalidation', async () => {
        const response = deferred();
        const fetchMock = vi.fn((_url: string, _options: RequestInit) => response.promise);
        vi.stubGlobal('fetch', fetchMock);
        const request = get('/profiles', {}, { profileScoped: false });

        invalidateProfileScopedRequests();
        response.resolve(jsonResponse({ items: [], profile_selection_required: true }));

        await expect(request).resolves.toEqual({ items: [], profile_selection_required: true });
        expect(fetchMock.mock.calls[0]?.[1]?.signal).toBeUndefined();
    });

    it('still honors caller cancellation for unscoped requests', async () => {
        const controller = new AbortController();
        vi.stubGlobal('fetch', vi.fn((_url, options) => new Promise((_resolve, reject) => {
            options.signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true });
        })));
        const request = get('/profiles', {}, { profileScoped: false, signal: controller.signal });
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });

        controller.abort();

        await rejection;
    });
});

describe('API failures', () => {
    it('preserves RFC 9457 fields, validation details and retry timing', async () => {
        const problem = {
            type: '/errors/validation',
            title: 'VALIDATION_ERROR',
            status: 429,
            detail: 'Fixture request rejected',
            trace_id: 'fixture-trace',
            errors: [{ field: 'name', code: 'length', message: 'Name is required' }],
        };
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify(problem), {
            status: 429,
            headers: { 'Content-Type': 'application/problem+json', 'Retry-After': '12' },
        })));

        const error = await get('/profiles').catch((failure) => failure);

        expect(error).toBeInstanceOf(ApiError);
        expect(error).toMatchObject({ detail: problem.detail, traceId: 'fixture-trace', retryAfter: 12 });
        expect(error.isRateLimited).toBe(true);
        expect(error.isValidation).toBe(true);
        expect(error.fieldError('name')).toEqual(problem.errors[0]);
    });

    it('turns transport failures into a useful API error', async () => {
        vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Fixture connection unavailable')));

        await expect(get('/media-items')).rejects.toMatchObject({
            name: 'ApiError',
            type: '/errors/network',
            status: 0,
            detail: 'Fixture connection unavailable',
        });
    });

    it('handles a non-JSON HTTP failure without masking its status', async () => {
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('unavailable', { status: 503, statusText: 'Service Unavailable' })));

        await expect(get('/media-items')).rejects.toMatchObject({ status: 503, title: 'HTTP_503', detail: 'Service Unavailable' });
    });
});
