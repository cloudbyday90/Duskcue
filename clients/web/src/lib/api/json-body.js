/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export async function boundedJson(response, maxBytes, signal) {
    if (!Number.isSafeInteger(maxBytes) || maxBytes < 1) throw new Error('Invalid JSON budget');
    signal?.throwIfAborted();
    const length = response.headers.get('Content-Length');
    if (length && /^\d+$/.test(length) && Number(length) > maxBytes) {
        await response.body?.cancel().catch(() => {});
        throw new Error('JSON budget exceeded');
    }
    if (!response.body) throw new Error('Missing JSON body');
    const reader = response.body.getReader();
    const abort = () => { reader.cancel().catch(() => {}); };
    signal?.addEventListener('abort', abort, { once: true });
    const decoder = new TextDecoder('utf-8', { fatal: true });
    let size = 0;
    let text = '';
    try {
        while (true) {
            signal?.throwIfAborted();
            const { value, done } = await reader.read();
            signal?.throwIfAborted();
            if (done) break;
            size += value.byteLength;
            if (size > maxBytes) throw new Error('JSON budget exceeded');
            text += decoder.decode(value, { stream: true });
        }
        text += decoder.decode();
        return JSON.parse(text);
    } catch (error) {
        await reader.cancel().catch(() => {});
        throw error;
    } finally {
        signal?.removeEventListener('abort', abort);
        reader.releaseLock();
    }
}
