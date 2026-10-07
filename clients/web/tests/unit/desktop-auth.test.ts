import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const mocks = vi.hoisted(() => ({
    scope: { isDesktop: true, server: { origin: 'https://first.invalid:48027' }, hasToken: true },
    persist: vi.fn(), clear: vi.fn(), login: vi.fn(), setup: vi.fn(), sessions: vi.fn(), logout: vi.fn(),
}));

vi.mock('../../src/lib/desktop/session.js', () => ({
    getDesktopSessionScope: () => mocks.scope,
    persistDesktopSession: mocks.persist,
    clearDesktopSession: mocks.clear,
}));
vi.mock('../../src/lib/api/auth.js', () => ({
    setup: mocks.setup, loginWithInvite: mocks.login, loginWithPassword: mocks.login,
    startWebauthnAuth: vi.fn(), finishWebauthnAuth: mocks.login,
    logout: mocks.logout, logoutAll: mocks.logout, listSessions: mocks.sessions,
}));

function deferred<T = any>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((settle) => { resolve = settle; });
    return { promise, resolve };
}

const user = { id: 'actual-account', display_name: 'Actual user', role: 'member', capabilities: ['play_media'], active_profile_id: 'profile-a' };
const cacheKey = `duskcue_user:${encodeURIComponent('https://first.invalid:48027')}`;
let values: Map<string, string>;

beforeEach(() => {
    vi.resetModules();
    vi.clearAllMocks();
    mocks.scope = { isDesktop: true, server: { origin: 'https://first.invalid:48027' }, hasToken: true };
    mocks.persist.mockResolvedValue({});
    mocks.clear.mockResolvedValue({});
    mocks.login.mockResolvedValue({ session_token: 'native-only-secret', user });
    mocks.sessions.mockResolvedValue({ items: [] });
    mocks.logout.mockResolvedValue({});
    values = new Map();
    vi.stubGlobal('localStorage', {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => { values.set(key, value); },
        removeItem: (key: string) => { values.delete(key); },
    });
});

describe('shared auth desktop persistence', () => {
    it('explicit revalidation recognizes its own definitive current-scope 401 clear', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        mocks.sessions.mockRejectedValueOnce(Object.assign(new Error('Expired'), { status: 401 }));
        const outcome = await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: mocks.scope.server.origin, userId: user.id } });
        expect(outcome).toMatchObject({ status: 'expired' });
        if (typeof outcome !== 'object') throw new Error('Expected structured revalidation');
        expect(outcome.isCurrent()).toBe(true);
        expect(get(auth)).toMatchObject({ user: null, isAuthenticated: false, loading: false });
        expect(values.has(cacheKey)).toBe(false);
        expect(mocks.clear).toHaveBeenCalledExactlyOnceWith(mocks.scope.server.origin);
        auth.setUser({ ...user, id: 'new-account' });
        expect(outcome.isCurrent()).toBe(false);
    });

    it.each([0, 503])('preserves a validated current cookie scope after a revalidation failure %s', async (status) => {
        mocks.scope = { isDesktop: false, server: null, hasToken: false };
        vi.stubGlobal('location', { origin: 'https://browser.invalid' });
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        mocks.sessions.mockRejectedValueOnce(Object.assign(new Error('Validation unavailable'), { status }));
        const outcome = await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: 'https://browser.invalid', userId: user.id } });
        expect(outcome).toMatchObject({ status: 'unavailable' });
        expect(get(auth)).toMatchObject({ user, isAuthenticated: true, loading: false });
        expect(JSON.parse(values.get('duskcue_user')!).id).toBe(user.id);
        expect(mocks.clear).not.toHaveBeenCalled();
    });

    it('preserves a newer successful account when an older session probe returns 401 late', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        const pending = deferred();
        mocks.sessions.mockImplementationOnce(async () => { await pending.promise; throw Object.assign(new Error('Old credential expired'), { status: 401 }); });
        const checking = auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: mocks.scope.server.origin, userId: user.id } });
        const newer = { ...user, id: 'new-account' };
        mocks.login.mockResolvedValueOnce({ session_token: 'new-account-token', user: newer });
        await auth.loginWithPassword({ username: 'fixture-new', password: 'fixture' });
        pending.resolve({});
        const outcome = await checking;
        expect(outcome).toMatchObject({ status: 'superseded' });
        expect(get(auth)).toMatchObject({ user: newer, isAuthenticated: true });
        expect(mocks.clear).not.toHaveBeenCalled();
        expect(JSON.parse(values.get(cacheKey)!).id).toBe(newer.id);
    });

    it('rechecks the auth revision after awaited credential clearing before clearing a newer user', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        const pending = deferred();
        mocks.sessions.mockRejectedValueOnce(Object.assign(new Error('Expired'), { status: 401 }));
        mocks.clear.mockReturnValueOnce(pending.promise);
        const checking = auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: mocks.scope.server.origin, userId: user.id } });
        await vi.waitFor(() => expect(mocks.clear).toHaveBeenCalledOnce());
        const newer = { ...user, id: 'new-account' };
        auth.setUser(newer);
        pending.resolve({});
        expect(await checking).toMatchObject({ status: 'superseded' });
        expect(get(auth)).toMatchObject({ user: newer, isAuthenticated: true });
        expect(JSON.parse(values.get(cacheKey)!).id).toBe(newer.id);
    });

    it('never clears a selected newer server after the old origin probe returns 401', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        const pending = deferred();
        mocks.sessions.mockImplementationOnce(async () => { await pending.promise; throw Object.assign(new Error('Expired'), { status: 401 }); });
        const checking = auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: mocks.scope.server.origin, userId: user.id } });
        mocks.scope.server = { origin: 'https://second.invalid:48027' };
        pending.resolve({});
        expect(await checking).toMatchObject({ status: 'superseded' });
        expect(mocks.clear).not.toHaveBeenCalled();
        expect(get(auth).isAuthenticated).toBe(true);
    });

    it('keeps a current authenticated account when revalidation succeeds without a readable cached summary', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        values.delete(cacheKey);
        expect(await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: mocks.scope.server.origin, userId: user.id } })).toMatchObject({ status: 'valid' });
        expect(get(auth)).toMatchObject({ user, isAuthenticated: true, loading: false });
        expect(mocks.clear).not.toHaveBeenCalled();
    });

    it('does not probe or clear a mismatched captured playback context', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.setUser(user);
        expect(await auth.checkSession({ revalidate: true, expectedContext: { serverOrigin: 'https://other.invalid:48027', userId: user.id } })).toMatchObject({ status: 'superseded' });
        expect(mocks.sessions).not.toHaveBeenCalled();
        expect(mocks.clear).not.toHaveBeenCalled();
        expect(get(auth)).toMatchObject({ user, isAuthenticated: true, loading: false });
    });

    it('awaits secure persistence before adopting the real server user and caches no credential', async () => {
        const writing = deferred();
        mocks.persist.mockReturnValue(writing.promise);
        const { auth } = await import('../../src/lib/stores/auth.js');
        const login = auth.loginWithPassword({ username: 'fixture', password: 'fixture' });
        await Promise.resolve();
        expect(get(auth).isAuthenticated).toBe(false);
        expect(mocks.persist).toHaveBeenCalledWith('native-only-secret', mocks.scope.server.origin);
        writing.resolve({});
        await login;
        expect(get(auth)).toMatchObject({ isAuthenticated: true, user });
        expect(JSON.parse(values.get(cacheKey)!)).toEqual(user);
        expect([...values.values()].join('')).not.toContain('native-only-secret');
        expect(values.has('duskcue_user')).toBe(false);
    });

    it('keeps a failed keyring write visible and does not adopt a new account', async () => {
        mocks.persist.mockRejectedValue(new Error('OS keyring is locked'));
        const { auth } = await import('../../src/lib/stores/auth.js');
        await expect(auth.loginWithInvite({ code: 'fixture' })).rejects.toThrow('locked');
        expect(get(auth)).toMatchObject({ isAuthenticated: false, user: null });
        expect(get(auth).error?.message).toContain('locked');
        expect(values.has(cacheKey)).toBe(false);
    });

    it('clears the newly persisted native credential if its real user summary cannot be cached', async () => {
        vi.stubGlobal('localStorage', {
            getItem: () => null,
            setItem: () => { throw new Error('User storage is unavailable'); },
            removeItem: () => {},
        });
        const { auth } = await import('../../src/lib/stores/auth.js');
        await expect(auth.loginWithPassword({ username: 'fixture', password: 'fixture' })).rejects.toThrow('unavailable');
        expect(mocks.persist).toHaveBeenCalledWith('native-only-secret', mocks.scope.server.origin);
        expect(mocks.clear).toHaveBeenCalledWith(mocks.scope.server.origin);
        expect(get(auth).isAuthenticated).toBe(false);
    });

    it('does not restore a user from another origin or authenticate before checking the session', async () => {
        values.set('duskcue_user', JSON.stringify({ id: 'other-origin-user' }));
        values.set(cacheKey, JSON.stringify(user));
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.init();
        expect(get(auth)).toMatchObject({ isAuthenticated: false, user });
        expect(await auth.checkSession()).toBe(true);
        expect(get(auth).isAuthenticated).toBe(true);
    });

    it('does not invent an account when a secure token lacks its scoped user summary', async () => {
        const { auth } = await import('../../src/lib/stores/auth.js');
        expect(await auth.checkSession()).toBe(false);
        expect(mocks.clear).toHaveBeenCalledWith(mocks.scope.server.origin);
        expect(get(auth).user).toBeNull();
    });

    it('clears the secure credential on 401 while retaining it after a network failure', async () => {
        values.set(cacheKey, JSON.stringify(user));
        const { auth } = await import('../../src/lib/stores/auth.js');
        mocks.sessions.mockRejectedValueOnce(Object.assign(new Error('Offline'), { status: 0 }));
        expect(await auth.checkSession()).toBe(false);
        expect(mocks.clear).not.toHaveBeenCalled();
        expect(values.has(cacheKey)).toBe(true);
        mocks.sessions.mockRejectedValueOnce(Object.assign(new Error('Expired'), { status: 401 }));
        expect(await auth.checkSession()).toBe(false);
        expect(mocks.clear).toHaveBeenCalledWith(mocks.scope.server.origin);
        expect(values.has(cacheKey)).toBe(false);
    });

    it('rejects a stale login after logout and clears only the selected origin', async () => {
        const response = deferred();
        mocks.login.mockReturnValue(response.promise);
        const { auth } = await import('../../src/lib/stores/auth.js');
        const login = auth.loginWithPassword({ username: 'fixture', password: 'fixture' });
        const rejection = expect(login).rejects.toMatchObject({ name: 'AbortError' });
        await auth.logout();
        response.resolve({ session_token: 'late-secret', user });
        await rejection;
        expect(mocks.persist).not.toHaveBeenCalled();
        expect(mocks.clear).toHaveBeenCalledWith(mocks.scope.server.origin);
        expect(get(auth).isAuthenticated).toBe(false);
    });

    it('rejects a stale login after the selected server changes', async () => {
        const response = deferred();
        mocks.login.mockReturnValue(response.promise);
        const { auth } = await import('../../src/lib/stores/auth.js');
        const login = auth.loginWithPassword({ username: 'fixture', password: 'fixture' });
        const rejection = expect(login).rejects.toMatchObject({ name: 'AbortError' });
        mocks.scope.server = { origin: 'https://second.invalid:48027' };
        response.resolve({ session_token: 'late-secret', user });
        await rejection;
        expect(mocks.persist).not.toHaveBeenCalled();
        expect(values.has(cacheKey)).toBe(false);
    });

    it('clears only memory on server selection and retains the old scoped summary and native credential', async () => {
        values.set(cacheKey, JSON.stringify(user));
        const { auth } = await import('../../src/lib/stores/auth.js');
        auth.init();
        await auth.checkSession();
        auth.resetForServerSelection();
        expect(get(auth)).toMatchObject({ isAuthenticated: false, user: null, loading: false });
        expect(values.has(cacheKey)).toBe(true);
        expect(mocks.clear).not.toHaveBeenCalled();
    });
});
