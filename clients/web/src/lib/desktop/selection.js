import { writable } from 'svelte/store';

export function createServerSelection({ selectServer, beforeSelect, onSelected, onSettled = (_result) => {} }) {
    const state = writable({ phase: 'idle', error: null, result: null });
    let pending = false;
    let disposed = false;

    async function connect(input) {
        if (pending || disposed) return null;
        let selectedResult = null;
        pending = true;
        state.set({ phase: 'checking', error: null, result: null });
        try {
            if (!await beforeSelect()) {
                state.set({ phase: 'idle', error: null, result: null });
                return null;
            }
            if (disposed) return null;
            state.set({ phase: 'selecting', error: null, result: null });
            const result = await selectServer({ ...input });
            selectedResult = result;
            if (disposed) return null;
            state.set({ phase: 'selected', error: null, result });
            await onSelected(result);
            return result;
        } catch (error) {
            if (!disposed) state.set({ phase: 'error', error, result: null });
            return null;
        } finally {
            pending = false;
            await onSettled(selectedResult);
        }
    }

    return { subscribe: state.subscribe, connect, dispose() { disposed = true; } };
}
