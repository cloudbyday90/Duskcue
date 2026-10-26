/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

const eventName = 'duskcue-session-context';
const storageKey = 'duskcue_session_context_notice';
export function announceSessionChange(origin) {
    if (typeof window === 'undefined' || !origin) return;
    const nonce = Array.from(crypto.getRandomValues(new Uint8Array(16)), (value) => value.toString(16).padStart(2, '0')).join('');
    const notice = { origin, nonce };
    window.dispatchEvent(new CustomEvent(eventName, { detail: notice }));
    try { const channel = new BroadcastChannel(eventName); channel.postMessage(notice); channel.close(); } catch {}
    try { localStorage.setItem(storageKey, JSON.stringify(notice)); } catch {}
}

export function listenSessionChanges(readOrigin, callback) {
    const seen = new Set();
    const receive = (notice) => {
        if (!notice || notice.origin !== readOrigin() || typeof notice.nonce !== 'string' || notice.nonce.length > 64 || seen.has(notice.nonce)) return;
        seen.add(notice.nonce);
        if (seen.size > 64) seen.delete(seen.values().next().value);
        callback();
    };
    const local = (event) => receive(event.detail);
    const storage = (event) => { if (event.key === storageKey && event.newValue?.length <= 2048) { try { receive(JSON.parse(event.newValue)); } catch {} } };
    let channel;
    try { channel = new BroadcastChannel(eventName); channel.onmessage = (event) => receive(event.data); } catch {}
    window.addEventListener(eventName, local); window.addEventListener('storage', storage);
    return () => { channel?.close(); window.removeEventListener(eventName, local); window.removeEventListener('storage', storage); };
}
