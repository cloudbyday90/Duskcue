import { afterEach, describe, expect, it, vi } from 'vitest';
import { createEventsStore } from '../../src/lib/stores/events.js';
import { clearServerOrigin, setServerOrigin } from '../../src/lib/api/core.js';

class FakeSource extends EventTarget {
    readyState = 0;
    onopen: any = null;
    onerror: any = null;
    close = vi.fn(() => { this.readyState = 2; });
    lastDispatcher: any;
    addEventListener(type: string, listener: any) { this.lastDispatcher = listener; super.addEventListener(type, listener); }
    open() { this.readyState = 1; this.onopen?.(new Event('open')); }
    message(type: string, data: unknown, id = 'one') { this.dispatchEvent(new MessageEvent(type, { data: JSON.stringify(data), lastEventId: id })); }
}

afterEach(() => clearServerOrigin());

describe('event facade source and listener ownership', () => {
    it('uses the selected API origin and reattaches a type after its final handler is removed', () => {
        setServerOrigin('https://selected.example');
        const source = new FakeSource();
        const createSource = vi.fn((_options: unknown): any => source);
        const store = createEventsStore({ createSource });
        const first = vi.fn(); const second = vi.fn();
        const off = store.on('notification', first);
        store.connect(); source.open(); source.message('notification', { title: 'First' }); off();
        source.message('notification', { title: 'Ignored' });
        store.on('notification', second); source.message('notification', { title: 'Second' }, '');
        expect(createSource.mock.calls[0][0]).toMatchObject({ url: 'https://selected.example/api/v1/events' });
        expect(first).toHaveBeenCalledOnce(); expect(second).toHaveBeenCalledExactlyOnceWith({ title: 'Second' }, expect.any(MessageEvent));
        expect(store.getState()?.lastEventId).toBe(''); store.disconnect();
    });

    it('ignores stale dispatcher and open callbacks after reconnect while preserving current subscribers', () => {
        const sources = [new FakeSource(), new FakeSource()]; let index = 0;
        const store = createEventsStore({ createSource: (_options: unknown): any => sources[index++] });
        const handler = vi.fn(); store.on('storyboard_progress', handler); store.connect();
        const staleDispatcher = sources[0].lastDispatcher; const staleOpen = sources[0].onopen;
        store.disconnect(); store.connect();
        staleDispatcher(new MessageEvent('storyboard_progress', { data: '{"progress":99}', lastEventId: 'old' })); staleOpen();
        expect(handler).not.toHaveBeenCalled(); expect(store.getState()?.readyState).toBe('connecting');
        sources[1].open(); sources[1].message('storyboard_progress', { progress: 20 }, 'new');
        expect(handler).toHaveBeenCalledExactlyOnceWith({ progress: 20 }, expect.any(MessageEvent));
        expect(store.getState()).toMatchObject({ readyState: 'connected', lastEventId: 'new' }); store.disconnect();
    });

    it('terminal errors release the current source and allow a clean reconnect', () => {
        const sources = [new FakeSource(), new FakeSource()]; let index = 0;
        const store = createEventsStore({ createSource: (_options: unknown): any => sources[index++] });
        const handler = vi.fn(); store.on('migration_progress', handler); store.connect();
        sources[0].readyState = 2; sources[0].onerror(new Event('error'));
        expect(store.getState()).toMatchObject({ readyState: 'disconnected', error: 'connection_failed' });
        store.connect(); sources[1].open(); sources[1].message('migration_progress', { progress: 1 });
        expect(handler).toHaveBeenCalledOnce(); store.disconnect();
    });
});
