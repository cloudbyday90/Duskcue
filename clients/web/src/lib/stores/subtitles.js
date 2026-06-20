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
    getSubtitleSettings as apiGetSettings,
    updateSubtitleSettings as apiUpdateSettings,
    updateSubtitleProviderSettings as apiUpdateProviders,
} from '../api/subtitles.js';

function createSubtitleSettingsStore() {
    const { subscribe, set, update } = writable({
        settings: null,
        loading: false,
        saving: false,
        error: null,
    });

    return {
        subscribe,

        async fetch() {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const settings = await apiGetSettings();
                set({ settings, loading: false, saving: false, error: null });
                return settings;
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
                throw err;
            }
        },

        async saveSettings(data) {
            update((s) => ({ ...s, saving: true, error: null }));
            try {
                const settings = await apiUpdateSettings(data);
                set({ settings, loading: false, saving: false, error: null });
                return settings;
            } catch (err) {
                update((s) => ({ ...s, saving: false, error: err }));
                throw err;
            }
        },

        async saveProviders(data) {
            update((s) => ({ ...s, saving: true, error: null }));
            try {
                const settings = await apiUpdateProviders(data);
                set({ settings, loading: false, saving: false, error: null });
                return settings;
            } catch (err) {
                update((s) => ({ ...s, saving: false, error: err }));
                throw err;
            }
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },
    };
}

export const subtitleSettings = createSubtitleSettingsStore();

export const subtitleSettingsLoading = derived(subtitleSettings, ($s) => $s.loading);
export const subtitleSettingsSaving = derived(subtitleSettings, ($s) => $s.saving);
export const subtitleSettingsError = derived(subtitleSettings, ($s) => $s.error);
