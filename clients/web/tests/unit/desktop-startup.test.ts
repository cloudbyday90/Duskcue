import { describe, expect, it, vi } from 'vitest';
import { createShellStartup } from '../../src/lib/desktop/startup.js';
vi.mock('../../src/lib/desktop/tauri.js', () => ({ isTauriDesktop: () => false }));

function pending<T>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((accept) => { resolve = accept; });
    return { promise, resolve };
}

const desktop = { isDesktop: true, server: { origin: 'http://127.0.0.1:48027' }, requiresServerSelection: false, hasToken: true };

function harness(initializeSession: () => Promise<any>) {
    const auth = { init: vi.fn(), checkSession: vi.fn(async () => true) };
    const onChange = vi.fn((_state: any) => {});
    const readSetup = vi.fn(async () => ({ setup_required: false }));
    const startup = createShellStartup({ auth, onChange, initializeSession, sessionScope: () => desktop, readSetup });
    return { startup, auth, onChange, readSetup };
}

describe('selected-server shell startup', () => {
    it('performs no authentication or setup request before a server is selected', async () => {
        const h = harness(async () => ({ ...desktop, server: null, requiresServerSelection: true, hasToken: false }));
        expect(await h.startup.start()).toMatchObject({ ready: false, desktop: { requiresServerSelection: true } });
        expect(h.auth.init).not.toHaveBeenCalled();
        expect(h.auth.checkSession).not.toHaveBeenCalled();
        expect(h.readSetup).not.toHaveBeenCalled();
        h.startup.dispose();
    });

    it('awaits the restored native session before setup and publishing a ready shell', async () => {
        const session = pending<boolean>();
        const h = harness(async () => desktop);
        h.auth.checkSession.mockReturnValueOnce(session.promise);
        const boot = h.startup.start();
        await vi.waitFor(() => expect(h.auth.checkSession).toHaveBeenCalledOnce());
        expect(h.readSetup).not.toHaveBeenCalled();
        session.resolve(true);
        expect(await boot).toMatchObject({ ready: true, error: null });
        expect(h.auth.init).toHaveBeenCalledOnce();
        expect(h.readSetup).toHaveBeenCalledOnce();
        h.startup.dispose();
    });

    it('ignores a late older boot instead of adopting the preceding server', async () => {
        const old = pending<any>();
        const initialize = vi.fn(async () => desktop).mockReturnValueOnce(old.promise);
        const h = harness(initialize);
        const older = h.startup.start();
        await h.startup.start();
        old.resolve({ ...desktop, server: { origin: 'https://old.example' } });
        expect(await older).toBeNull();
        expect(h.auth.init).toHaveBeenCalledOnce();
        expect(h.onChange).toHaveBeenLastCalledWith(expect.objectContaining({ ready: true, desktop }));
        h.startup.dispose();
    });

    it('does not issue requests after unmount while secure restoration is pending', async () => {
        const native = pending<any>();
        const h = harness(() => native.promise);
        const boot = h.startup.start();
        h.startup.dispose();
        native.resolve(desktop);
        expect(await boot).toBeNull();
        expect(h.auth.init).not.toHaveBeenCalled();
        expect(h.readSetup).not.toHaveBeenCalled();
    });

    it('keeps native restoration failure recoverable without calling a default origin', async () => {
        const initialize = vi.fn(async () => desktop).mockRejectedValueOnce(new Error('Keyring unavailable'));
        const h = harness(initialize);
        expect(await h.startup.start()).toMatchObject({ ready: false, error: expect.any(Error) });
        expect(h.readSetup).not.toHaveBeenCalled();
        expect(await h.startup.start()).toMatchObject({ ready: true, error: null });
        h.startup.dispose();
    });
});
