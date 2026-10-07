import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getArtwork } from '../../src/lib/api/artwork.js';
import {
    clearBearerToken,
    clearServerOrigin,
    invalidateProfileScopedRequests,
    setBearerToken,
    setServerOrigin,
} from '../../src/lib/api/core.js';

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

describe('authenticated artwork', () => {
    it('uses the selected server, bearer credentials and actual blob body', async () => {
        setServerOrigin('https://fixture-server.invalid');
        setBearerToken('fixture-token');
        const blob = new Blob(['fixture-image'], { type: 'image/webp' });
        const fetchMock = vi.fn().mockResolvedValue(new Response(blob, { headers: { 'Content-Type': 'image/webp' } }));
        vi.stubGlobal('fetch', fetchMock);

        const result = await getArtwork('title/with space', 'backdrop', 'w780');

        expect(fetchMock).toHaveBeenCalledWith('https://fixture-server.invalid/api/v1/items/title%2Fwith%20space/artwork/backdrop?size=w780', expect.objectContaining({
            headers: expect.objectContaining({ Authorization: 'Bearer fixture-token' }),
            credentials: 'same-origin',
            signal: expect.any(AbortSignal),
        }));
        expect(result).toBeInstanceOf(Blob);
        expect(await result.text()).toBe('fixture-image');
        expect(result.type).toBe('image/webp');
    });

    it('keeps browser artwork on the current origin without a bearer token', async () => {
        const fetchMock = vi.fn().mockResolvedValue(new Response(new Blob(['fixture'])));
        vi.stubGlobal('fetch', fetchMock);

        await getArtwork('fixture-item');

        expect(fetchMock).toHaveBeenCalledWith('/api/v1/items/fixture-item/artwork/poster?size=w342', expect.objectContaining({
            credentials: 'same-origin',
            headers: { Accept: 'application/json' },
        }));
    });

    it('cancels pending artwork on a profile change without wrapping AbortError', async () => {
        vi.stubGlobal('fetch', vi.fn((_url, options) => new Promise((_resolve, reject) => {
            options.signal.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError')), { once: true });
        })));
        const request = getArtwork('fixture-item');
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });

        invalidateProfileScopedRequests();

        await rejection;
    });

    it('discards a blob decoded after its profile scope changed', async () => {
        let resolveBlob!: (value: Blob) => void;
        let markStarted!: () => void;
        const started = new Promise<void>((resolve) => { markStarted = resolve; });
        const body = new Promise<Blob>((resolve) => { resolveBlob = resolve; });
        vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
            status: 200,
            ok: true,
            headers: new Headers({ 'Content-Type': 'image/webp' }),
            blob: () => {
                markStarted();
                return body;
            },
        }));
        const request = getArtwork('fixture-item');
        const rejection = expect(request).rejects.toMatchObject({ name: 'AbortError' });
        await started;

        invalidateProfileScopedRequests();
        resolveBlob(new Blob(['stale fixture']));

        await rejection;
    });
});
