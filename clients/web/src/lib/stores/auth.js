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

import { writable, derived, get } from 'svelte/store';
import {
    setup as apiSetup,
    loginWithInvite as apiLoginInvite,
    loginWithPassword as apiLoginPassword,
    startWebauthnAuth as apiStartWebauthnAuth,
    finishWebauthnAuth as apiFinishWebauthnAuth,
    logout as apiLogout,
    logoutAll as apiLogoutAll,
    listSessions as apiListSessions,
} from '../api/auth.js';
import { getDeviceIdentity } from '../device/identity.js';
import { persistDesktopSession, clearDesktopSession, getDesktopSessionScope } from '../desktop/session.js';

const USER_STORAGE_KEY = 'duskcue_user';

function withWebDeviceIdentity(data) {
    const deviceId = getDeviceIdentity();
    if (!deviceId) return data;

    return {
        ...data,
        device_id: data.device_id ?? deviceId,
        device_name: data.device_name ?? (getDesktopSessionScope().isDesktop ? 'Desktop' : 'Web Browser'),
        client_name: data.client_name ?? (getDesktopSessionScope().isDesktop ? 'Duskcue Desktop' : 'Duskcue Web'),
        client_platform: data.client_platform ?? (getDesktopSessionScope().isDesktop ? 'desktop' : 'web'),
    };
}

function createAuthStore() {
    let revision = 0;
    const { subscribe, set, update } = writable({
        user: null,
        isAuthenticated: false,
        loading: false,
        error: null,
    });

    function userStorageKey() {
        const scope = getDesktopSessionScope();
        return scope.isDesktop ? scope.server?.origin ? `${USER_STORAGE_KEY}:${encodeURIComponent(scope.server.origin)}` : null : USER_STORAGE_KEY;
    }

    function persistUser(user) {
        if (typeof localStorage === 'undefined') return;
        const key = userStorageKey();
        if (!key) return;
        if (user) {
            const cached = Object.fromEntries(['id', 'username', 'display_name', 'role', 'capabilities', 'has_all_library_access', 'active_profile_id', 'profile_selection_required', 'locale']
                .filter((field) => user[field] !== undefined).map((field) => [field, user[field]]));
            localStorage.setItem(key, JSON.stringify(cached));
        } else {
            localStorage.removeItem(key);
        }
    }

    function restoreUser() {
        if (typeof localStorage === 'undefined') return null;
        const key = userStorageKey();
        if (!key) return null;
        const stored = localStorage.getItem(key);
        if (!stored) return null;
        try {
            const user = JSON.parse(stored);
            return user && typeof user.id === 'string' ? user : null;
        } catch {
            return null;
        }
    }

    async function handleSessionResult(result, expectedRevision, origin) {
        if (revision !== expectedRevision || origin !== getDesktopSessionScope().server?.origin) throw new DOMException('Authentication changed.', 'AbortError');
        await persistDesktopSession(result.session_token, origin);
        if (revision !== expectedRevision || origin !== getDesktopSessionScope().server?.origin) throw new DOMException('Authentication changed.', 'AbortError');
        const user = result.user || null;
        try { persistUser(user); } catch (error) {
            await clearDesktopSession(origin);
            throw error;
        }
        set({ user, isAuthenticated: !!user, loading: false, error: null });
    }

    return {
        subscribe,

        init() {
            const cached = restoreUser();
            if (cached) {
                update((s) => ({ ...s, user: cached, isAuthenticated: !getDesktopSessionScope().isDesktop }));
            }
        },

        async checkSession({ revalidate = false, expectedContext = null } = {}) {
            const expectedRevision = revision;
            const desktop = getDesktopSessionScope();
            const desktopOrigin = desktop.server?.origin;
            const userId = get({ subscribe }).user?.id || null;
            const origin = desktopOrigin || globalThis.location?.origin;
            const current = () => revision === expectedRevision
                && desktopOrigin === getDesktopSessionScope().server?.origin
                && (!revalidate || get({ subscribe }).user?.id === userId);
            const outcome = (status, error = null) => ({
                status, error,
                isCurrent: () => revision === expectedRevision
                    && desktopOrigin === getDesktopSessionScope().server?.origin
                    && (status === 'expired'
                        ? !get({ subscribe }).isAuthenticated && !get({ subscribe }).user
                        : get({ subscribe }).user?.id === userId),
            });
            if (revalidate) {
                let expectedOrigin;
                try { expectedOrigin = new URL(expectedContext?.serverOrigin).origin; } catch {}
                if (!userId || !get({ subscribe }).isAuthenticated || (expectedContext && (expectedContext.userId !== userId || expectedOrigin !== origin))) {
                    return outcome('superseded');
                }
            }
            update((s) => ({ ...s, loading: true }));
            try {
                if (!revalidate && desktop.isDesktop && (!desktop.server || !desktop.hasToken)) {
                    set({ user: null, isAuthenticated: false, loading: false, error: null });
                    return false;
                }
                await apiListSessions();
                if (!current()) return revalidate ? outcome('superseded') : false;
                const cached = restoreUser();
                if (!cached) {
                    if (revalidate) {
                        update((s) => ({ ...s, loading: false, error: null }));
                        return outcome('valid');
                    }
                    await clearDesktopSession(desktopOrigin);
                    if (!current()) return false;
                    set({ user: null, isAuthenticated: false, loading: false, error: null });
                    return false;
                }
                update((s) => ({
                    ...s,
                    user: cached,
                    isAuthenticated: true,
                    loading: false,
                    error: null,
                }));
                if (revalidate && cached.id !== userId) {
                    revision += 1;
                    return outcome('superseded');
                }
                return revalidate ? outcome('valid') : true;
            } catch (err) {
                if (!current()) return revalidate ? outcome('superseded') : false;
                if (revalidate && err.status !== 401) {
                    update((s) => ({ ...s, loading: false, error: err }));
                    return outcome('unavailable', err);
                }
                if (revalidate) {
                    let clearingError = null;
                    try { await clearDesktopSession(desktopOrigin); } catch (error) { clearingError = error; }
                    if (!current()) return outcome('superseded');
                    try { persistUser(null); } catch (error) { clearingError ||= error; }
                    set({ user: null, isAuthenticated: false, loading: false, error: clearingError || err });
                    return outcome('expired', clearingError || err);
                }
                if (!desktop.isDesktop || err.status === 401) {
                    persistUser(null);
                    try { await clearDesktopSession(desktopOrigin); } catch (storageError) {
                        if (!current()) return false;
                        set({ user: null, isAuthenticated: false, loading: false, error: storageError });
                        return false;
                    }
                }
                if (!current()) return false;
                set({ user: null, isAuthenticated: false, loading: false, error: err });
                return false;
            }
        },

        async setup(data) {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiSetup(data);
                await handleSessionResult(result, expectedRevision, origin);
                return result;
            } catch (err) {
                if (revision === expectedRevision) update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithInvite(data) {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiLoginInvite(withWebDeviceIdentity(data));
                await handleSessionResult(result, expectedRevision, origin);
                return result;
            } catch (err) {
                if (revision === expectedRevision) update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithPassword(data) {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiLoginPassword(withWebDeviceIdentity(data));
                await handleSessionResult(result, expectedRevision, origin);
                return result;
            } catch (err) {
                if (revision === expectedRevision) update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithPasskey(getCredential) {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const startResult = await apiStartWebauthnAuth({});
                const challengeId = startResult.challenge_id;
                const options = startResult.public_key_options || startResult;
                const credential = await getCredential(options);
                const result = await apiFinishWebauthnAuth(
                    withWebDeviceIdentity({ credential }),
                    challengeId,
                );
                await handleSessionResult(result, expectedRevision, origin);
                return result;
            } catch (err) {
                if (revision === expectedRevision) update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async logout() {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            try {
                await apiLogout();
            } catch {
            }
            if (revision !== expectedRevision || origin !== getDesktopSessionScope().server?.origin) return;
            persistUser(null);
            let error = null;
            try { await clearDesktopSession(origin); } catch (err) { error = err; }
            if (revision === expectedRevision) set({ user: null, isAuthenticated: false, loading: false, error });
        },

        async logoutAll() {
            const expectedRevision = ++revision;
            const origin = getDesktopSessionScope().server?.origin;
            try {
                await apiLogoutAll();
            } catch {
            }
            if (revision !== expectedRevision || origin !== getDesktopSessionScope().server?.origin) return;
            persistUser(null);
            let error = null;
            try { await clearDesktopSession(origin); } catch (err) { error = err; }
            if (revision === expectedRevision) set({ user: null, isAuthenticated: false, loading: false, error });
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },

        resetForServerSelection() {
            revision += 1;
            set({ user: null, isAuthenticated: false, loading: false, error: null });
        },

        setUser(user) {
            if (get({ subscribe }).user?.id !== user?.id) revision += 1;
            persistUser(user);
            update((s) => ({ ...s, user, isAuthenticated: !!user }));
        },
    };
}

export const auth = createAuthStore();

export const isAuthenticated = derived(auth, ($auth) => $auth.isAuthenticated);

export const currentUser = derived(auth, ($auth) => $auth.user);

export const authLoading = derived(auth, ($auth) => $auth.loading);

export const authError = derived(auth, ($auth) => $auth.error);

export const userRole = derived(auth, ($auth) => $auth.user?.role || null);

export const userCapabilities = derived(auth, ($auth) => $auth.user?.capabilities || []);

export function userHasCapability(user, capability) {
    if (!user) return false;
    if (user.role === 'owner') return true;
    return (user.capabilities || []).includes(capability);
}

export function userHasAnyCapability(user, capabilities) {
    return capabilities.some((capability) => userHasCapability(user, capability));
}

export function hasCapability(capability) {
    return derived(auth, ($auth) => userHasCapability($auth.user, capability));
}
