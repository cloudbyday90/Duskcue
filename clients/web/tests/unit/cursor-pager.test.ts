import { describe, expect, it, vi } from 'vitest';
import { createCursorPager } from '../../src/lib/browsing/cursor-pager.js';

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((settle) => { resolve = settle; });
    return { promise, resolve };
}

describe('per-view cursor pager', () => {
    it('refreshes authoritative content after a mutation instead of replaying stale cached pages', async () => {
        const loader = vi.fn().mockResolvedValueOnce({ items: [{ id: 'old' }], has_more: false }).mockResolvedValueOnce({ items: [{ id: 'new' }], has_more: false });
        const pager = createCursorPager(loader);
        await pager.load({ library_id: 'owned' });

        await pager.refresh();

        expect(loader).toHaveBeenCalledTimes(2);
        expect(loader.mock.calls[1][0]).toMatchObject({ library_id: 'owned' });
        expect(pager.getState().items).toEqual([{ id: 'new' }]);
        pager.dispose();
    });
    it('replays complete pages with filters and deduplicates IDs', async () => {
        const loader = vi.fn()
            .mockResolvedValueOnce({ items: [{ id: 'a', title: 'First' }], cursor: 'next', has_more: true })
            .mockResolvedValueOnce({ items: [{ id: 'a', title: 'Updated' }, { id: 'b' }], cursor: null, has_more: false });
        const pager = createCursorPager(loader, 12);

        await pager.load({ type: 'movie', favorite: false }, 4);

        expect(loader).toHaveBeenCalledTimes(2);
        expect(loader.mock.calls[1][0]).toEqual({ type: 'movie', favorite: false, limit: 12, cursor: 'next' });
        expect(pager.getState()).toMatchObject({ items: [{ id: 'a', title: 'Updated' }, { id: 'b' }], loadedPages: 2, hasMore: false, loading: false });
        pager.dispose();
    });

    it('cancels superseded work and cannot publish an old query result', async () => {
        const first = deferred<{ items: { id: string }[] }>();
        const loader = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce({ items: [{ id: 'new' }], has_more: false });
        const pager = createCursorPager(loader);

        const old = pager.load({ type: 'movie' });
        const signal = loader.mock.calls[0][1].signal;
        await pager.load({ type: 'series' });
        first.resolve({ items: [{ id: 'old' }] });
        await old;

        expect(signal.aborted).toBe(true);
        expect(pager.getState().items).toEqual([{ id: 'new' }]);
        pager.dispose();
    });

    it('retains loaded cards after an append failure and retries that cursor', async () => {
        const loader = vi.fn()
            .mockResolvedValueOnce({ items: [{ id: 'a' }], cursor: 'next', has_more: true })
            .mockRejectedValueOnce(new Error('Fixture page unavailable'))
            .mockResolvedValueOnce({ items: [{ id: 'b' }], cursor: null, has_more: false });
        const pager = createCursorPager(loader);
        await pager.load({ sort: 'title' });
        await pager.next();

        expect(pager.getState()).toMatchObject({ items: [{ id: 'a' }], loadedPages: 1, cursor: 'next', error: null, loadingMore: false });
        expect(pager.getState().moreError).toBeInstanceOf(Error);

        await pager.retry();

        expect(loader.mock.calls[2][0]).toMatchObject({ sort: 'title', cursor: 'next' });
        expect(pager.getState()).toMatchObject({ items: [{ id: 'a' }, { id: 'b' }], moreError: null, loadedPages: 2 });
        pager.dispose();
    });

    it('distinguishes initial failure from empty results and can retry', async () => {
        const loader = vi.fn().mockRejectedValueOnce(new Error('Fixture failure')).mockResolvedValueOnce({ items: [], cursor: null, has_more: false });
        const pager = createCursorPager(loader);
        await pager.load({});
        expect(pager.getState().error).toBeInstanceOf(Error);

        await pager.retry();

        expect(pager.getState()).toMatchObject({ items: [], loadedPages: 1, error: null, loading: false });
        pager.dispose();
    });

    it('restores a shorter visible prefix and can reveal the already loaded next page', async () => {
        const loader = vi.fn().mockResolvedValueOnce({ items: [{ id: 'a' }], cursor: 'next', has_more: true }).mockResolvedValueOnce({ items: [{ id: 'b' }], has_more: false });
        const pager = createCursorPager(loader);
        await pager.load({}, 2);
        await pager.load({}, 1);
        expect(pager.getState().items).toEqual([{ id: 'a' }]);

        await pager.next();

        expect(pager.getState().items).toEqual([{ id: 'a' }, { id: 'b' }]);
        expect(loader).toHaveBeenCalledTimes(2);
        pager.dispose();
    });

    it('stops invalid cursor cycles and keeps the first usable page', async () => {
        const loader = vi.fn().mockResolvedValueOnce({ items: [{ id: 'a' }], cursor: 'same', has_more: true }).mockResolvedValueOnce({ items: [{ id: 'b' }], cursor: 'same', has_more: true });
        const pager = createCursorPager(loader);

        await pager.load({}, 3);

        expect(loader).toHaveBeenCalledTimes(2);
        expect(pager.getState().items).toEqual([{ id: 'a' }]);
        expect(pager.getState().moreError).toBeInstanceOf(Error);
        pager.dispose();
    });
});
