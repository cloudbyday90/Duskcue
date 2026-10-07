import { afterEach, describe, expect, it, vi } from 'vitest';
import { createEventTransport } from '../../src/lib/events/transport.js';
import { createEventStreamParser } from '../../src/lib/events/framing.js';
import { clearBearerToken, clearServerOrigin, setBearerToken, setServerOrigin } from '../../src/lib/api/core.js';

const url = 'https://selected.example/api/v1/events';
const encoder = new TextEncoder();
const sources: any[] = [];

function stream() {
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const cancel = vi.fn();
    const body = new ReadableStream<Uint8Array>({ start(value) { controller = value; }, cancel });
    return { body, controller, cancel };
}

function source(options: any = {}) {
    setServerOrigin('https://selected.example');
    setBearerToken('fixture-session');
    const result = createEventTransport({ url, authenticated: true, ...options });
    sources.push(result);
    return result as any;
}

async function settle() {
    for (let index = 0; index < 200; index += 1) await Promise.resolve();
}

afterEach(() => {
    for (const value of sources.splice(0)) value.close();
    clearBearerToken(); clearServerOrigin(); vi.useRealTimers();
});

describe('SSE framing', () => {
    it('handles CRLF split across chunks, CR, LF, comments, literal fields and multi-line data', () => {
        const events: any[] = [];
        const retry = vi.fn((_value: string) => {});
        const parser = createEventStreamParser({ onEvent: (event) => events.push(event), onRetry: retry });
        for (const text of ['\uFEFF: keep-alive\r', '\nevent: notification\rdata: first\ndata:  second\r\nretry: 5000\r\nretry: +5\nretry: ５\nID: ignored\nid: one\n\n']) parser.push(text);
        expect(events).toEqual([{ type: 'notification', data: 'first\n second', lastEventId: 'one' }]);
        expect(retry).toHaveBeenCalledExactlyOnceWith('5000');
    });

    it('inherits IDs, ignores NUL IDs and resets empty IDs even without data', () => {
        const events: any[] = [];
        const ids: string[] = [];
        const parser = createEventStreamParser({ onEvent: (event) => events.push(event), onId: (id) => ids.push(id) });
        parser.push('id: one\ndata: first\n\nid: bad\0id\ndata: second\n\nid\n\ndata\n\n');
        expect(events.map((event) => [event.data, event.lastEventId])).toEqual([['first', 'one'], ['second', 'one'], ['', '']]);
        expect(ids).toEqual(['one', 'one', '', '']);
    });

    it('discards incomplete EOF data and bounds both incomplete lines and multi-line frames', () => {
        const event = vi.fn((_event: unknown) => {});
        const parser = createEventStreamParser({ onEvent: event });
        parser.push('data: unfinished\n'); parser.dispose();
        expect(event).not.toHaveBeenCalled();
        expect(() => createEventStreamParser({ maxFrameChars: 20 }).push('data:' + 'x'.repeat(21))).toThrow(/frame limit/);
        expect(() => createEventStreamParser({ maxFrameChars: 20 }).push('data: one\ndata: two\ndata: three\n')).toThrow(/frame limit/);
    });
});

describe('selected-origin SSE transport', () => {
    it('keeps native cookie EventSource and builds no bearer URL', () => {
        const native = { close: vi.fn() };
        const Native = vi.fn(function (_url: string) { return native; });
        const fetcher = vi.fn();
        expect(createEventTransport({ url: '/api/v1/events', authenticated: false, EventSourceClass: Native as any, fetcher })).toBe(native);
        expect(Native).toHaveBeenCalledExactlyOnceWith('/api/v1/events');
        expect(fetcher).not.toHaveBeenCalled();
    });

    it('authenticates only selected-origin headers and decodes real UTF-8 chunks into named events', async () => {
        const readable = stream();
        const fetcher = vi.fn(async (_url: string, _options: RequestInit) => new Response(readable.body, { headers: { 'Content-Type': 'text/event-stream; charset=utf-8' } }));
        const transport = source({ fetcher });
        const events: MessageEvent[] = [];
        transport.addEventListener('notification', (event: MessageEvent) => events.push(event));
        await settle();
        const options = fetcher.mock.calls[0][1];
        expect(fetcher.mock.calls[0][0]).toBe(url);
        expect(url).not.toContain('fixture-session');
        expect(options).toMatchObject({ headers: { Authorization: 'Bearer fixture-session', Accept: 'text/event-stream' }, credentials: 'omit', redirect: 'error', cache: 'no-store' });
        const bytes = encoder.encode('\uFEFFid: one\nevent: notification\ndata: Café 🎬\ndata: next\n\n');
        for (const byte of bytes) readable.controller.enqueue(Uint8Array.of(byte));
        await settle();
        expect(events).toHaveLength(1);
        expect(events[0]).toMatchObject({ type: 'notification', data: 'Café 🎬\nnext', lastEventId: 'one', origin: 'https://selected.example' });
        transport.close();
        expect(options.signal?.aborted).toBe(true);
        await settle();
        expect(readable.cancel).toHaveBeenCalledOnce();
    });

    it('rejects a foreign request before fetch without putting a credential in any URL', () => {
        const fetcher = vi.fn();
        expect(() => source({ url: 'https://foreign.example/api/v1/events', fetcher })).toThrow();
        expect(fetcher).not.toHaveBeenCalled();
    });

    it('reconnects after EOF using parsed retry and Last-Event-ID, then clears the ID on an empty id frame', async () => {
        vi.useFakeTimers();
        const first = stream(); const second = stream(); const third = stream();
        const fetcher = vi.fn(async (_url: string, _options: RequestInit) => new Response([first, second, third][fetcher.mock.calls.length - 1].body, { headers: { 'Content-Type': 'text/event-stream' } }));
        const transport = source({ fetcher });
        await settle();
        first.controller.enqueue(encoder.encode('retry: 5000\nid: one\ndata: first\n\n')); first.controller.close();
        await settle();
        expect(transport.readyState).toBe(0);
        await vi.advanceTimersByTimeAsync(4999);
        expect(fetcher).toHaveBeenCalledOnce();
        await vi.advanceTimersByTimeAsync(1);
        expect(fetcher.mock.calls[1][1].headers).toMatchObject({ 'Last-Event-ID': 'one' });
        second.controller.enqueue(encoder.encode('id\n\n')); second.controller.close();
        await settle(); await vi.advanceTimersByTimeAsync(5000);
        expect(fetcher.mock.calls[2][1].headers).not.toHaveProperty('Last-Event-ID');
    });

    it('close clears its reconnect timer and cannot start another request', async () => {
        vi.useFakeTimers();
        const body = stream();
        const fetcher = vi.fn(async () => new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } }));
        const transport = source({ fetcher });
        await settle(); body.controller.close(); await settle();
        transport.close(); await vi.advanceTimersByTimeAsync(10000);
        expect(fetcher).toHaveBeenCalledOnce(); expect(transport.readyState).toBe(2);
    });

    it('retries a transient network failure only while its captured scope remains current', async () => {
        vi.useFakeTimers();
        let current = true;
        const fetcher = vi.fn(async () => { throw new Error('fixture network failure'); });
        const transport = source({ fetcher, isCurrent: () => current });
        await settle();
        expect(transport.readyState).toBe(0);
        await vi.advanceTimersByTimeAsync(3000);
        expect(fetcher).toHaveBeenCalledTimes(2);
        current = false;
        await vi.advanceTimersByTimeAsync(3000);
        expect(fetcher).toHaveBeenCalledTimes(2);
        expect(transport.readyState).toBe(2);
    });

    it('rejects a foreign response origin before opening or decoding its body', async () => {
        const body = stream();
        const response = new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } });
        Object.defineProperty(response, 'url', { value: 'https://foreign.example/api/v1/events' });
        const transport = source({ fetcher: async () => response });
        const opened = vi.fn(); const error = vi.fn((_event: any) => {});
        transport.onopen = opened; transport.onerror = error;
        await settle();
        expect(opened).not.toHaveBeenCalled(); expect(body.cancel).toHaveBeenCalledOnce();
        expect(transport.readyState).toBe(2); expect(error.mock.calls[0][0].error.code).toBe('FOREIGN_RESPONSE');
    });

    it('does not dispatch an incomplete EOF frame or send its uncommitted ID on reconnect', async () => {
        vi.useFakeTimers();
        const first = stream(); const second = stream();
        const fetcher = vi.fn(async (_url: string, _options: RequestInit) => new Response(fetcher.mock.calls.length === 1 ? first.body : second.body, { headers: { 'Content-Type': 'text/event-stream' } }));
        const transport = source({ fetcher }); const message = vi.fn(); transport.onmessage = message;
        await settle(); first.controller.enqueue(encoder.encode('id: uncommitted\ndata: incomplete\n')); first.controller.close();
        await settle(); await vi.advanceTimersByTimeAsync(3000);
        expect(message).not.toHaveBeenCalled(); expect(fetcher.mock.calls[1][1].headers).not.toHaveProperty('Last-Event-ID');
    });

    it('cancels a response that arrives after close and cannot emit a late open or message', async () => {
        const body = stream();
        let resolve!: (response: Response) => void;
        const fetcher = vi.fn((_url: string, _options: RequestInit) => new Promise<Response>((accept) => { resolve = accept; }));
        const transport = source({ fetcher });
        const opened = vi.fn(); const message = vi.fn(); transport.onopen = opened; transport.onmessage = message;
        await settle(); transport.close();
        resolve(new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } }));
        await settle();
        expect(fetcher.mock.calls[0][1].signal?.aborted).toBe(true);
        expect(body.cancel).toHaveBeenCalledOnce(); expect(opened).not.toHaveBeenCalled(); expect(message).not.toHaveBeenCalled();
    });

    it('rejects late responses after selected server or bearer scope changes', async () => {
        const body = stream();
        let resolve!: (response: Response) => void;
        const transport = source({ fetcher: () => new Promise<Response>((accept) => { resolve = accept; }) });
        const opened = vi.fn(); transport.onopen = opened;
        await settle(); setServerOrigin('https://other.example'); setBearerToken('other-fixture');
        resolve(new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } }));
        await settle();
        expect(opened).not.toHaveBeenCalled(); expect(body.cancel).toHaveBeenCalledOnce(); expect(transport.readyState).toBe(2);
    });

    it.each([401, 403, 204])('closes HTTP %s without reconnect or sensitive diagnostics', async (status) => {
        vi.useFakeTimers();
        const fetcher = vi.fn(async () => new Response(null, { status }));
        const transport = source({ fetcher });
        const error = vi.fn((_event: any) => {}); transport.onerror = error;
        await settle(); await vi.advanceTimersByTimeAsync(10000);
        expect(transport.readyState).toBe(2); expect(fetcher).toHaveBeenCalledOnce();
        expect(error.mock.calls[0][0].error).toMatchObject({ code: 'HTTP_ERROR', status });
        expect(error.mock.calls[0][0].error.message).not.toContain('fixture-session');
    });

    it('rejects a non-SSE response and oversized frame rather than retaining unbounded data', async () => {
        const wrong = source({ fetcher: async () => new Response('html', { headers: { 'Content-Type': 'text/html' } }) });
        await settle(); expect(wrong.readyState).toBe(2);
        const body = stream();
        const oversized = source({ fetcher: async () => new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } }), maxFrameChars: 20 });
        const error = vi.fn((_event: any) => {}); oversized.onerror = error;
        await settle(); body.controller.enqueue(encoder.encode('data:' + 'x'.repeat(21))); await settle();
        expect(oversized.readyState).toBe(2); expect(error.mock.calls[0][0].error.code).toBe('FRAME_LIMIT');
    });

    it('stops chunk dispatch as soon as a listener closes its source', async () => {
        const body = stream();
        const transport = source({ fetcher: async () => new Response(body.body, { headers: { 'Content-Type': 'text/event-stream' } }) });
        const message = vi.fn(() => transport.close()); transport.onmessage = message;
        await settle(); body.controller.enqueue(encoder.encode('data: one\n\ndata: two\n\n')); await settle();
        expect(message).toHaveBeenCalledOnce(); expect(transport.readyState).toBe(2);
    });
});
