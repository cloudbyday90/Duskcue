/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { isTauriDesktop } from '../desktop/tauri.js';

async function loadNativeWindow() {
    if (!isTauriDesktop()) return null;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return getCurrentWindow();
}

export function createFullscreenController({ element, document = null, loadNative = null, onChange }) {
    document ||= globalThis.document;
    const resolveNative = loadNative || loadNativeWindow;
    let native = null;
    let unlisten = null;
    let disposed = false;
    let ownedNative = false;
    let operation = null;
    let state = { fullscreen: false, pending: false, available: typeof element?.requestFullscreen === 'function', error: null };

    function publish(update) {
        state = { ...state, ...update };
        if (!disposed) onChange(state);
    }

    async function refresh() {
        const fullscreen = document?.fullscreenElement === element || Boolean(native && await native.isFullscreen());
        publish({ fullscreen });
        return fullscreen;
    }

    const changed = () => { refresh().catch((error) => publish({ error })); };
    document?.addEventListener('fullscreenchange', changed);
    const ready = resolveNative().then(async (window) => {
        if (disposed) return;
        native = window;
        if (native) {
            publish({ available: true });
            const stop = await native.onResized(changed);
            if (disposed) stop();
            else unlisten = stop;
        }
        await refresh();
    }).catch((error) => publish({ error }));

    async function enter() {
        let browserError;
        if (element?.requestFullscreen) {
            try {
                await element.requestFullscreen();
                if (document?.fullscreenElement === element) return;
            } catch (error) { browserError = error; }
        }
        await ready;
        if (!native) throw browserError || new Error('Fullscreen is unavailable');
        await native.setFullscreen(true);
        if (!await native.isFullscreen()) throw new Error('Fullscreen request did not complete');
        ownedNative = true;
    }

    async function exit() {
        if (document?.fullscreenElement === element) await document.exitFullscreen();
        if (native && await native.isFullscreen()) await native.setFullscreen(false);
        ownedNative = false;
        await refresh();
    }

    function run(action) {
        if (operation) return operation;
        publish({ pending: true, error: null });
        operation = action().then(refresh).catch((error) => {
            publish({ error });
            throw error;
        }).finally(() => {
            operation = null;
            publish({ pending: false });
        });
        return operation;
    }

    return {
        ready,
        getState: () => state,
        toggle: () => run(state.fullscreen ? exit : enter),
        async exit() {
            if (operation) await operation.catch(() => {});
            return run(exit);
        },
        dispose() {
            disposed = true;
            document?.removeEventListener('fullscreenchange', changed);
            unlisten?.();
            Promise.resolve(operation).catch(() => {}).then(async () => {
                if (document?.fullscreenElement === element) await document.exitFullscreen();
                if (ownedNative) await native.setFullscreen(false);
            }).catch(() => {});
        },
    };
}
