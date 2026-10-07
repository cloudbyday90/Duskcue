import { describe, expect, it, vi } from 'vitest';
import { createNavigationGuard } from '../../src/lib/navigation/guard.js';

describe('unsaved preference navigation guard', () => {
    it('allows clean transitions without prompting and unregisters a mounted form', async () => {
        const guard = createNavigationGuard();
        const confirmDiscard = vi.fn();
        const unregister = guard.register({ isDirty: () => false, confirmDiscard, discard: vi.fn() });
        expect(await guard.requestProfileTransition()).toBe(true);
        expect(confirmDiscard).not.toHaveBeenCalled();
        unregister();
        expect(guard.hasUnsavedChanges()).toBe(false);
    });

    it('retains changes when Keep editing or Escape rejects the dialog', async () => {
        const guard = createNavigationGuard();
        const discard = vi.fn();
        guard.register({ isDirty: () => true, confirmDiscard: async () => false, discard });
        expect(guard.hasUnsavedChanges()).toBe(true);
        expect(await guard.requestNavigationTransition()).toBe(false);
        expect(discard).not.toHaveBeenCalled();
    });

    it('discards before allowing an explicitly approved profile transition', async () => {
        const guard = createNavigationGuard();
        let dirty = true;
        const discard = vi.fn(() => { dirty = false; });
        guard.register({ isDirty: () => dirty, confirmDiscard: async () => true, discard });
        expect(await guard.requestProfileTransition()).toBe(true);
        expect(discard).toHaveBeenCalledOnce();
        expect(guard.hasUnsavedChanges()).toBe(false);
    });

    it('blocks transitions while saving, including if the form becomes busy during confirmation', async () => {
        const guard = createNavigationGuard();
        let busy = true;
        const discard = vi.fn();
        const confirmDiscard = vi.fn(async () => { busy = true; return true; });
        guard.register({ isDirty: () => true, isBusy: () => busy, confirmDiscard, discard });
        expect(guard.isTransitionBlocked()).toBe(true);
        expect(await guard.requestProfileTransition()).toBe(false);
        expect(confirmDiscard).not.toHaveBeenCalled();
        busy = false;
        expect(await guard.requestProfileTransition()).toBe(false);
        expect(discard).not.toHaveBeenCalled();
    });

    it('does not let one discard confirmation approve two distinct transitions', async () => {
        const guard = createNavigationGuard();
        let resolve!: (allowed: boolean) => void;
        const pending = new Promise<boolean>((settle) => { resolve = settle; });
        const discard = vi.fn();
        const confirmDiscard = vi.fn(() => pending);
        guard.register({ isDirty: () => true, confirmDiscard, discard });
        const navigation = guard.requestNavigationTransition();
        expect(await guard.requestProfileTransition()).toBe(false);
        expect(confirmDiscard).toHaveBeenCalledOnce();
        resolve(true);
        expect(await navigation).toBe(true);
        expect(discard).toHaveBeenCalledOnce();
    });

    it('rejects a pending transition if its form unmounts before approval', async () => {
        const guard = createNavigationGuard();
        let resolve!: (allowed: boolean) => void;
        const discard = vi.fn();
        const unregister = guard.register({ isDirty: () => true, confirmDiscard: () => new Promise((settle) => { resolve = settle; }), discard });
        const transition = guard.requestProfileTransition();
        unregister();
        resolve(true);
        expect(await transition).toBe(false);
        expect(discard).not.toHaveBeenCalled();
    });
});
