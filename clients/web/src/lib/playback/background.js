/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

async function loadNativeWindow() {
    if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return null;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
}

export function createPlaybackBackground({ document = globalThis.document, loadNative = loadNativeWindow, onChange = (_state) => {}, clock = globalThis, pollMs = 400 } = {}) {
    if (!Number.isFinite(pollMs) || pollMs < 250 || pollMs > 500) throw new RangeError('Background polling must stay between 250 and 500 milliseconds');
    let disposed = false;
    let active = false;
    let native = null;
    let revision = 0;
    let pending = null;
    let poll = null;
    const subscriptions = new Set();
    let state = { documentHidden: document?.hidden === true, nativeBackground: true, nativeKnown: false, nativeMinimized: null, nativeVisible: null, error: null };

    function getState() { return { ...state }; }
    function publish(update) {
        if (disposed) return;
        state = { ...state, ...update, documentHidden: document?.hidden === true };
        onChange(getState());
    }
    function clearPoll() {
        if (poll !== null) clock.clearTimeout(poll);
        poll = null;
    }
    function schedule() {
        clearPoll();
        if (!disposed && active && native && !pending) poll = clock.setTimeout(() => { poll = null; void refreshNative(); }, pollMs);
    }
    function unknown(error = null) {
        publish({ nativeKnown: false, nativeBackground: true, nativeMinimized: null, nativeVisible: null, error });
    }
    function refreshNative() {
        if (disposed) return Promise.resolve(false);
        if (!native) return Promise.resolve(state.nativeKnown);
        if (pending) return pending.result;
        clearPoll();
        const version = revision;
        const request = {};
        let finish;
        let valid = false;
        request.result = new Promise((resolve) => { finish = resolve; });
        pending = request;
        const timeout = clock.setTimeout(() => {
            if (disposed || pending !== request) return;
            revision += 1;
            unknown(new Error('Native background state could not be confirmed'));
            finish(false);
        }, 1500);
        request.timeout = timeout;
        request.finish = finish;
        Promise.resolve().then(() => Promise.allSettled([native.isMinimized(), native.isVisible()])).then((results) => {
            if (disposed || revision !== version) return;
            const [minimized, visible] = results.map((result) => {
                if (result.status === 'rejected') throw result.reason;
                return result.value;
            });
            if (typeof minimized !== 'boolean' || typeof visible !== 'boolean') throw new TypeError('Native background state is invalid');
            valid = true;
            publish({ nativeKnown: true, nativeBackground: minimized || !visible, nativeMinimized: minimized, nativeVisible: visible, error: null });
        }).catch((error) => {
            if (!disposed && revision === version) unknown(error);
        }).finally(() => {
            clock.clearTimeout(timeout);
            if (pending === request) pending = null;
            finish(valid && !disposed && revision === version);
            if (!disposed && revision !== version) void refreshNative();
            else schedule();
        });
        return request.result;
    }
    function nativeChanged() {
        if (disposed) return;
        revision += 1;
        unknown();
        void refreshNative();
    }
    function documentChanged() { publish({}); }
    async function subscribe(method) {
        try {
            const stop = await native[method](nativeChanged);
            if (typeof stop !== 'function') throw new TypeError('Native background subscription is invalid');
            if (disposed) stop();
            else subscriptions.add(stop);
        } catch (error) {
            if (!disposed) publish({ error });
        }
    }

    document?.addEventListener('visibilitychange', documentChanged);
    publish({});
    const ready = Promise.resolve().then(loadNative).then(async (window) => {
        if (disposed) return;
        native = window;
        if (!native) {
            publish({ nativeKnown: true, nativeBackground: false, nativeMinimized: false, nativeVisible: true, error: null });
            return;
        }
        void subscribe('onFocusChanged');
        void subscribe('onResized');
        await refreshNative();
    }).catch((error) => unknown(error));

    return {
        ready,
        getState,
        setActive(enabled) {
            if (disposed || active === (enabled === true)) return;
            active = enabled === true;
            clearPoll();
            if (!active || !native && state.nativeKnown) return;
            revision += 1;
            unknown();
            void ready.then(() => { if (!disposed && active) void refreshNative(); });
        },
        async canAdvance({ signal = undefined } = {}) {
            await ready;
            if (disposed || signal?.aborted) return false;
            if (pending && !await pending.result) return false;
            if (disposed || signal?.aborted) return false;
            if (native && !await refreshNative()) return false;
            if (disposed || signal?.aborted) return false;
            documentChanged();
            return state.nativeKnown && !state.nativeBackground && !state.documentHidden;
        },
        dispose() {
            if (disposed) return;
            disposed = true;
            revision += 1;
            clearPoll();
            if (pending) { clock.clearTimeout(pending.timeout); pending.finish(false); }
            document?.removeEventListener('visibilitychange', documentChanged);
            for (const stop of subscriptions) { try { stop(); } catch {} }
            subscriptions.clear();
        },
    };
}
