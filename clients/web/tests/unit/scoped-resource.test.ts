import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { createScopedResource } from '../../src/lib/browsing/scoped-resource.js';

function deferred<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((settle) => { resolve = settle; });
    return { promise, resolve };
}

describe('per-view scoped metadata', () => {
    it('aborts old scope reads and rejects their late completion', async () => {
        const first = deferred<{ id: string }>();
        const loader = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce({ id: 'new' });
        const resource = createScopedResource(loader);
        const old = resource.load('old');
        const signal = loader.mock.calls[0][1].signal;

        await resource.load('new');
        first.resolve({ id: 'old' });
        await old;

        expect(signal.aborted).toBe(true);
        expect(get(resource)).toEqual({ item: { id: 'new' }, loading: false, error: null });
        resource.dispose();
    });

    it('keeps failed metadata distinct and retries the same scope', async () => {
        const loader = vi.fn().mockRejectedValueOnce(new Error('Fixture metadata unavailable')).mockResolvedValueOnce({ id: 'owned' });
        const resource = createScopedResource(loader);
        await resource.load('owned');
        expect(get(resource).error).toBeInstanceOf(Error);

        await resource.retry();

        expect(loader.mock.calls.map(([id]) => id)).toEqual(['owned', 'owned']);
        expect(get(resource).item).toEqual({ id: 'owned' });
        resource.dispose();
    });

    it('does not publish content after disposal', async () => {
        const result = deferred<{ id: string }>();
        const loader = vi.fn().mockReturnValue(result.promise);
        const resource = createScopedResource(loader);
        const pending = resource.load('owned');

        resource.dispose();
        result.resolve({ id: 'owned' });
        await pending;

        expect(loader.mock.calls[0][1].signal.aborted).toBe(true);
        expect(get(resource).item).toBeNull();
    });
});
