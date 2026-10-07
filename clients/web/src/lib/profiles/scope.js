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

const CHANNEL_NAME = 'duskcue-profile-scope';
const STORAGE_KEY = 'duskcue-profile-scope-event';

function contextKey(context) {
    if (!context?.userId || !context.serverOrigin) return null;
    try { return `${new URL(context.serverOrigin).origin}\u0000${context.userId}`; } catch { return null; }
}

export function createProfileScopeSync({ readContext, target = globalThis.window, createChannel = (name) => typeof BroadcastChannel === 'undefined' ? null : new BroadcastChannel(name) }) {
    let channel = null;
    let onChange = null;
    let listening = false;
    let generation = 0;
    const revisions = new Set();

    function receive(payload) {
        if (!listening || !payload || payload.type !== 'profile-scope-changed'
            || typeof payload.user_id !== 'string' || typeof payload.profile_id !== 'string'
            || typeof payload.server_origin !== 'string' || typeof payload.revision !== 'string'
            || !payload.revision || revisions.has(payload.revision)) return;
        const current = contextKey(readContext());
        const supplied = contextKey({ userId: payload.user_id, serverOrigin: payload.server_origin });
        if (!current || current !== supplied) return;
        revisions.add(payload.revision);
        if (revisions.size > 64) revisions.delete(revisions.values().next().value);
        onChange?.(payload);
    }

    function onStorage(event) {
        if (event.key !== STORAGE_KEY || !event.newValue) return;
        try { receive(JSON.parse(event.newValue)); } catch {}
    }

    function dispose() {
        generation += 1;
        listening = false;
        channel?.close();
        channel = null;
        target?.removeEventListener('storage', onStorage);
        onChange = null;
        revisions.clear();
    }

    function start(callback) {
        dispose();
        if (!target) return dispose;
        listening = true;
        onChange = callback;
        const currentGeneration = generation;
        try {
            channel = createChannel(CHANNEL_NAME);
            channel?.addEventListener('message', (event) => { if (currentGeneration === generation) receive(event.data); });
        } catch { channel = null; }
        target.addEventListener('storage', onStorage);
        return dispose;
    }

    function publish({ userId, profileId }) {
        const context = readContext();
        if (!target || !contextKey(context) || userId !== context.userId || typeof profileId !== 'string') return;
        const payload = {
            type: 'profile-scope-changed', user_id: userId, profile_id: profileId,
            server_origin: new URL(context.serverOrigin).origin,
            revision: globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random()}`,
        };
        try { channel?.postMessage(payload); } catch {}
        try {
            target.localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
            target.localStorage.removeItem(STORAGE_KEY);
        } catch {}
    }

    return { start, publish, dispose };
}
