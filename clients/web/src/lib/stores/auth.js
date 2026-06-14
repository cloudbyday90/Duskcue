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

import { writable, derived } from 'svelte/store';
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

const USER_STORAGE_KEY = 'duskcue_user';

function createAuthStore() {
    const { subscribe, set, update } = writable({
        user: null,
        isAuthenticated: false,
        loading: false,
        error: null,
    });

    function persistUser(user) {
        if (typeof localStorage === 'undefined') return;
        if (user) {
            localStorage.setItem(USER_STORAGE_KEY, JSON.stringify(user));
        } else {
            localStorage.removeItem(USER_STORAGE_KEY);
        }
    }

    function restoreUser() {
        if (typeof localStorage === 'undefined') return null;
        const stored = localStorage.getItem(USER_STORAGE_KEY);
        if (!stored) return null;
        try {
            return JSON.parse(stored);
        } catch {
            return null;
        }
    }

    function handleSessionResult(result) {
        const user = result.user || null;
        persistUser(user);
        set({ user, isAuthenticated: !!user, loading: false, error: null });
    }

    return {
        subscribe,

        init() {
            const cached = restoreUser();
            if (cached) {
                update((s) => ({ ...s, user: cached, isAuthenticated: true }));
            }
        },

        async checkSession() {
            update((s) => ({ ...s, loading: true }));
            try {
                await apiListSessions();
                const cached = restoreUser();
                update((s) => ({
                    ...s,
                    user: cached,
                    isAuthenticated: true,
                    loading: false,
                    error: null,
                }));
                return true;
            } catch {
                persistUser(null);
                set({ user: null, isAuthenticated: false, loading: false, error: null });
                return false;
            }
        },

        async setup(data) {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiSetup(data);
                handleSessionResult(result);
                return result;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithInvite(data) {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiLoginInvite(data);
                handleSessionResult(result);
                return result;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithPassword(data) {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const result = await apiLoginPassword(data);
                handleSessionResult(result);
                return result;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async loginWithPasskey(getCredential) {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const startResult = await apiStartWebauthnAuth({});
                const challengeId = startResult.challenge_id;
                const options = startResult.public_key_options || startResult;
                const credential = await getCredential(options);
                const result = await apiFinishWebauthnAuth(credential, challengeId);
                handleSessionResult(result);
                return result;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async logout() {
            try {
                await apiLogout();
            } catch {
            }
            persistUser(null);
            set({ user: null, isAuthenticated: false, loading: false, error: null });
        },

        async logoutAll() {
            try {
                await apiLogoutAll();
            } catch {
            }
            persistUser(null);
            set({ user: null, isAuthenticated: false, loading: false, error: null });
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },

        setUser(user) {
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

export function hasCapability(capability) {
    return derived(auth, ($auth) => {
        if (!$auth.user) return false;
        if ($auth.user.role === 'owner') return true;
        return ($auth.user.capabilities || []).includes(capability);
    });
}
