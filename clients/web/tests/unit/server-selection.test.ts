import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { createServerSelection } from '../../src/lib/desktop/selection.js';

const input = { input: 'server.example', networkMode: 'exposed' };
const snapshot = { isDesktop: true, server: { origin: 'https://server.example:48027', network_mode: 'exposed' }, hasToken: false, requiresServerSelection: false };

describe('desktop server selection orchestration', () => {
    it('checks unsaved changes before issuing native commands and bootstraps only after selection', async () => {
        const calls: string[] = [];
        const controller = createServerSelection({
            beforeSelect: async () => { calls.push('guard'); return true; },
            selectServer: async (draft) => { calls.push('native'); expect(draft).toEqual(input); return snapshot; },
            onSelected: async (result) => { calls.push('bootstrap'); expect(result).toEqual(snapshot); expect(get(controller).phase).toBe('selected'); },
            onSettled: async (result) => { calls.push('settled'); expect(result).toEqual(snapshot); },
        });
        expect(await controller.connect(input)).toEqual(snapshot);
        expect(calls).toEqual(['guard', 'native', 'bootstrap', 'settled']);
    });

    it('keeps the original server when the unsaved guard is canceled', async () => {
        const selectServer = vi.fn(async (_draft) => snapshot);
        const onSelected = vi.fn(async (_result) => {});
        const controller = createServerSelection({ beforeSelect: async () => false, selectServer, onSelected });
        expect(await controller.connect(input)).toBeNull();
        expect(selectServer).not.toHaveBeenCalled();
        expect(onSelected).not.toHaveBeenCalled();
        expect(get(controller).phase).toBe('idle');
    });

    it('retains failed native error and draft values for retry without bootstrapping', async () => {
        const failure = new Error('The selected server is not ready.');
        const selectServer = vi.fn<(_draft: typeof input) => Promise<typeof snapshot>>(async (_draft) => { throw failure; });
        const onSelected = vi.fn(async (_result) => {});
        const onSettled = vi.fn(async (_result) => {});
        const controller = createServerSelection({ beforeSelect: async () => true, selectServer, onSelected, onSettled });
        expect(await controller.connect(input)).toBeNull();
        expect(get(controller)).toMatchObject({ phase: 'error', error: failure });
        expect(input).toEqual({ input: 'server.example', networkMode: 'exposed' });
        expect(onSelected).not.toHaveBeenCalled();
        expect(onSettled).toHaveBeenLastCalledWith(null);
        selectServer.mockResolvedValueOnce(snapshot);
        expect(await controller.connect(input)).toEqual(snapshot);
        expect(onSelected).toHaveBeenCalledOnce();
        expect(onSettled).toHaveBeenLastCalledWith(snapshot);
    });

    it('prevents a second native selection while the first operation is pending', async () => {
        let complete: (result: typeof snapshot) => void;
        const operation = new Promise<typeof snapshot>((resolve) => { complete = resolve; });
        const selectServer = vi.fn(async (_draft) => operation);
        const controller = createServerSelection({ beforeSelect: async () => true, selectServer, onSelected: async (_result) => {} });
        const first = controller.connect(input);
        await vi.waitFor(() => expect(selectServer).toHaveBeenCalledOnce());
        expect(get(controller).phase).toBe('selecting');
        expect(await controller.connect({ input: 'another.example', networkMode: 'local' })).toBeNull();
        complete(snapshot);
        expect(await first).toEqual(snapshot);
        expect(selectServer).toHaveBeenCalledOnce();
    });

    it('does not deliver a late selection to a destroyed component', async () => {
        let complete: (result: typeof snapshot) => void;
        const operation = new Promise<typeof snapshot>((resolve) => { complete = resolve; });
        const selectServer = vi.fn(async (_draft) => operation);
        const onSelected = vi.fn(async (_result) => {});
        const controller = createServerSelection({ beforeSelect: async () => true, selectServer, onSelected });
        const pending = controller.connect(input);
        await vi.waitFor(() => expect(selectServer).toHaveBeenCalledOnce());
        controller.dispose();
        complete(snapshot);
        expect(await pending).toBeNull();
        expect(onSelected).not.toHaveBeenCalled();
    });
});
