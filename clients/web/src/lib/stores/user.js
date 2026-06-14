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
    listSessions as apiListSessions,
    deleteSession as apiDeleteSession,
    signOutEverywhere as apiSignOutEverywhere,
    requestUserReauth as apiRequestReauth,
    requestReauthCode as apiRequestReauthCode,
    listPasskeys as apiListPasskeys,
    startPasskeyRegistration as apiStartPasskeyReg,
    finishPasskeyRegistration as apiFinishPasskeyReg,
    deletePasskey as apiDeletePasskey,
} from '../api/auth.js';

const PREFS_STORAGE_KEY = 'duskcue_prefs';

const DEFAULT_PREFS = {
    theme: 'dark',
    defaultLibraryId: null,
    rememberFilters: true,
    autoplay: true,
    subtitleLanguage: null,
    audioLanguage: null,
};

function loadPrefs() {
    if (typeof localStorage === 'undefined') return { ...DEFAULT_PREFS };
    const stored = localStorage.getItem(PREFS_STORAGE_KEY);
    if (!stored) return { ...DEFAULT_PREFS };
    try {
        return { ...DEFAULT_PREFS, ...JSON.parse(stored) };
    } catch {
        return { ...DEFAULT_PREFS };
    }
}

function savePrefs(prefs) {
    if (typeof localStorage === 'undefined') return;
    localStorage.setItem(PREFS_STORAGE_KEY, JSON.stringify(prefs));
}

function extractItems(response) {
    if (Array.isArray(response)) return response;
    if (response && Array.isArray(response.items)) return response.items;
    return [];
}

function createUserStore() {
    const { subscribe, set, update } = writable({
        sessions: [],
        sessionsLoading: false,
        passkeys: [],
        passkeysLoading: false,
        preferences: loadPrefs(),
        error: null,
    });

    return {
        subscribe,

        async fetchSessions() {
            update((s) => ({ ...s, sessionsLoading: true, error: null }));
            try {
                const response = await apiListSessions();
                update((s) => ({
                    ...s,
                    sessions: extractItems(response),
                    sessionsLoading: false,
                    error: null,
                }));
            } catch (err) {
                update((s) => ({ ...s, sessionsLoading: false, error: err }));
            }
        },

        async deleteSession(sessionId) {
            try {
                await apiDeleteSession(sessionId);
                update((s) => ({
                    ...s,
                    sessions: s.sessions.filter((sess) => sess.id !== sessionId),
                    error: null,
                }));
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async signOutEverywhere() {
            try {
                const result = await apiSignOutEverywhere();
                update((s) => ({ ...s, sessions: [], error: null }));
                return result;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async requestReauth() {
            try {
                const result = await apiRequestReauth();
                return result;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async requestReauthCode() {
            try {
                const result = await apiRequestReauthCode();
                return result;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async fetchPasskeys() {
            update((s) => ({ ...s, passkeysLoading: true, error: null }));
            try {
                const response = await apiListPasskeys();
                update((s) => ({
                    ...s,
                    passkeys: extractItems(response),
                    passkeysLoading: false,
                    error: null,
                }));
            } catch (err) {
                update((s) => ({ ...s, passkeysLoading: false, error: err }));
            }
        },

        async registerPasskey(data, getCredential) {
            try {
                const startResult = await apiStartPasskeyReg(data);
                const challengeId = startResult.challenge_id;
                const options = startResult.public_key_options || startResult;
                const credential = await getCredential(options);
                const result = await apiFinishPasskeyReg(credential, challengeId);
                return result;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async deletePasskey(passkeyId) {
            try {
                await apiDeletePasskey(passkeyId);
                update((s) => ({
                    ...s,
                    passkeys: s.passkeys.filter((pk) => pk.id !== passkeyId),
                    error: null,
                }));
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        updatePreferences(partial) {
            update((s) => {
                const preferences = { ...s.preferences, ...partial };
                savePrefs(preferences);
                return { ...s, preferences };
            });
        },

        resetPreferences() {
            savePrefs(DEFAULT_PREFS);
            update((s) => ({ ...s, preferences: { ...DEFAULT_PREFS } }));
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },
    };
}

export const user = createUserStore();

export const sessions = derived(user, ($user) => $user.sessions);

export const passkeys = derived(user, ($user) => $user.passkeys);

export const preferences = derived(user, ($user) => $user.preferences);

export const userError = derived(user, ($user) => $user.error);
