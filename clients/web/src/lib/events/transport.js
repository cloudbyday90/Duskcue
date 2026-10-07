/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { streamRequestHeaders, requiresAuthenticatedMediaSource } from '../api/core.js';
import { createEventStreamParser } from './framing.js';

export const EVENT_SOURCE_CONNECTING = 0;
export const EVENT_SOURCE_OPEN = 1;
export const EVENT_SOURCE_CLOSED = 2;

export class EventTransportError extends Error {
    constructor(code, status = 0) { super('The event stream could not continue.'); this.code = code; this.status = status; }
}

export function createEventTransport({
    url, headers = streamRequestHeaders, authenticated = requiresAuthenticatedMediaSource(),
    fetcher = globalThis.fetch, EventSourceClass = globalThis.EventSource,
    isCurrent = () => true, setTimer = setTimeout, clearTimer = clearTimeout,
    maxFrameChars = 1024 * 1024,
}) {
    if (!authenticated) {
        if (!EventSourceClass) throw new EventTransportError('UNAVAILABLE');
        return new EventSourceClass(url);
    }
    const initialHeaders = headers(url);
    if (!initialHeaders.Authorization || !fetcher) throw new EventTransportError('UNAVAILABLE');
    const origin = new URL(url, globalThis.location?.origin).origin;
    const target = new EventTarget();
    let readyState = EVENT_SOURCE_CONNECTING;
    let generation = 0;
    let retryMs = 3000;
    let lastEventId = '';
    let timer;
    let controller;
    let reader;
    let parser;
    const source = {
        get readyState() { return readyState; },
        onopen: null, onerror: null, onmessage: null,
        addEventListener: target.addEventListener.bind(target),
        removeEventListener: target.removeEventListener.bind(target),
        close() {
            readyState = EVENT_SOURCE_CLOSED;
            generation += 1;
            if (timer !== undefined) clearTimer(timer);
            timer = undefined;
            controller?.abort();
            reader?.cancel().catch(() => {});
            parser?.dispose();
            lastEventId = '';
        },
    };

    function current(token) {
        if (readyState === EVENT_SOURCE_CLOSED || token !== generation) return false;
        let valid = false;
        try { valid = isCurrent() && headers(url).Authorization === initialHeaders.Authorization; } catch {}
        if (!valid) source.close();
        return valid;
    }

    function emit(event) {
        target.dispatchEvent(event);
        try { source[`on${event.type}`]?.(event); } catch {}
    }

    function fail(error, reconnect, token) {
        if (!current(token)) return;
        readyState = reconnect ? EVENT_SOURCE_CONNECTING : EVENT_SOURCE_CLOSED;
        const event = new Event('error');
        Object.defineProperty(event, 'error', { value: error });
        emit(event);
        if (!reconnect || !current(token)) { if (!reconnect) source.close(); return; }
        timer = setTimer(() => { timer = undefined; if (current(token)) connect(); }, retryMs);
    }

    async function connect() {
        const token = ++generation;
        if (!current(token)) return;
        const requestController = new AbortController();
        controller = requestController;
        let localReader;
        let localParser;
        try {
            const requestHeaders = { ...headers(url), Accept: 'text/event-stream' };
            if (lastEventId) requestHeaders['Last-Event-ID'] = Array.from(new TextEncoder().encode(lastEventId), (byte) => String.fromCharCode(byte)).join('');
            const response = await fetcher(url, { headers: requestHeaders, signal: requestController.signal, credentials: 'omit', cache: 'no-store', redirect: 'error' });
            if (!current(token)) { response.body?.cancel().catch(() => {}); return; }
            if (response.url && new URL(response.url).origin !== origin) {
                response.body?.cancel().catch(() => {});
                fail(new EventTransportError('FOREIGN_RESPONSE'), false, token);
                return;
            }
            if (response.status !== 200) {
                response.body?.cancel().catch(() => {});
                fail(new EventTransportError('HTTP_ERROR', response.status), response.status === 408 || response.status === 429 || response.status >= 500, token);
                return;
            }
            if (response.headers.get('content-type')?.split(';')[0].trim().toLowerCase() !== 'text/event-stream' || !response.body) {
                response.body?.cancel().catch(() => {});
                fail(new EventTransportError('INVALID_RESPONSE'), false, token);
                return;
            }
            localReader = response.body.getReader();
            reader = localReader;
            localParser = createEventStreamParser({
                maxFrameChars,
                onId(id) { if (current(token)) lastEventId = id; },
                onRetry(value) { if (current(token)) retryMs = Math.min(60000, Math.max(250, Number(value))); },
                onEvent(event) { if (current(token)) emit(new MessageEvent(event.type, { data: event.data, lastEventId: event.lastEventId, origin })); },
            });
            parser = localParser;
            readyState = EVENT_SOURCE_OPEN;
            emit(new Event('open'));
            const decoder = new TextDecoder('utf-8', { ignoreBOM: true });
            while (current(token)) {
                const chunk = await localReader.read();
                if (!current(token)) return;
                if (chunk.done) break;
                for (let offset = 0; offset < chunk.value.length && current(token); offset += 16384) {
                    localParser.push(decoder.decode(chunk.value.subarray(offset, offset + 16384), { stream: true }));
                }
            }
            fail(new EventTransportError('STREAM_ENDED'), true, token);
        } catch (error) {
            fail(new EventTransportError(error?.code === 'FRAME_LIMIT' ? 'FRAME_LIMIT' : 'NETWORK_ERROR'), error?.code !== 'FRAME_LIMIT', token);
        } finally {
            localParser?.dispose();
            requestController.abort();
            localReader?.cancel().catch(() => {});
            localReader?.releaseLock();
            if (reader === localReader) reader = null;
            if (parser === localParser) parser = null;
        }
    }

    Promise.resolve().then(connect);
    return source;
}
