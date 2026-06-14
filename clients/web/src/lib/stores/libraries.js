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
    listLibraries as apiListLibraries,
    getLibrary as apiGetLibrary,
    createLibrary as apiCreateLibrary,
    updateLibrary as apiUpdateLibrary,
    deleteLibrary as apiDeleteLibrary,
    scanLibrary as apiScanLibrary,
    listLibraryPaths as apiListLibraryPaths,
    createLibraryPath as apiCreateLibraryPath,
    updateLibraryPath as apiUpdateLibraryPath,
    deleteLibraryPath as apiDeleteLibraryPath,
} from '../api/libraries.js';

function extractItems(response) {
    if (Array.isArray(response)) return response;
    if (response && Array.isArray(response.items)) return response.items;
    return [];
}

function createLibrariesStore() {
    const { subscribe, set, update } = writable({
        items: [],
        currentLibraryId: null,
        currentLibrary: null,
        paths: {},
        scanning: {},
        loading: false,
        error: null,
    });

    return {
        subscribe,

        async fetch() {
            update((s) => ({ ...s, loading: true, error: null }));
            try {
                const response = await apiListLibraries();
                const items = extractItems(response);
                update((s) => ({ ...s, items, loading: false, error: null }));
            } catch (err) {
                update((s) => ({ ...s, loading: false, error: err }));
            }
        },

        async selectLibrary(libraryId) {
            update((s) => ({ ...s, currentLibraryId: libraryId, currentLibrary: null }));
            try {
                const library = await apiGetLibrary(libraryId);
                update((s) => ({ ...s, currentLibrary: library, error: null }));
            } catch (err) {
                update((s) => ({ ...s, error: err }));
            }
        },

        clearSelection() {
            update((s) => ({
                ...s,
                currentLibraryId: null,
                currentLibrary: null,
            }));
        },

        async create(data) {
            try {
                const library = await apiCreateLibrary(data);
                update((s) => ({
                    ...s,
                    items: [...s.items, library],
                    error: null,
                }));
                return library;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async update(libraryId, data) {
            try {
                const updated = await apiUpdateLibrary(libraryId, data);
                update((s) => ({
                    ...s,
                    items: s.items.map((lib) =>
                        lib.id === libraryId ? { ...lib, ...updated } : lib,
                    ),
                    currentLibrary:
                        s.currentLibraryId === libraryId
                            ? { ...s.currentLibrary, ...updated }
                            : s.currentLibrary,
                    error: null,
                }));
                return updated;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async remove(libraryId) {
            try {
                await apiDeleteLibrary(libraryId);
                update((s) => ({
                    ...s,
                    items: s.items.filter((lib) => lib.id !== libraryId),
                    currentLibraryId:
                        s.currentLibraryId === libraryId ? null : s.currentLibraryId,
                    currentLibrary:
                        s.currentLibraryId === libraryId ? null : s.currentLibrary,
                    error: null,
                }));
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async scan(libraryId, mode = 'full') {
            update((s) => ({
                ...s,
                scanning: { ...s.scanning, [libraryId]: true },
                error: null,
            }));
            try {
                const result = await apiScanLibrary(libraryId, { mode });
                update((s) => ({
                    ...s,
                    scanning: { ...s.scanning, [libraryId]: false },
                    error: null,
                }));
                return result;
            } catch (err) {
                update((s) => ({
                    ...s,
                    scanning: { ...s.scanning, [libraryId]: false },
                    error: err,
                }));
                throw err;
            }
        },

        async fetchPaths(libraryId) {
            try {
                const response = await apiListLibraryPaths(libraryId);
                const items = extractItems(response);
                update((s) => ({
                    ...s,
                    paths: { ...s.paths, [libraryId]: items },
                    error: null,
                }));
                return items;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async createPath(libraryId, data) {
            try {
                const path = await apiCreateLibraryPath(libraryId, data);
                update((s) => {
                    const existing = s.paths[libraryId] || [];
                    return {
                        ...s,
                        paths: { ...s.paths, [libraryId]: [...existing, path] },
                        error: null,
                    };
                });
                return path;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async updatePath(libraryId, pathId, data) {
            try {
                const updated = await apiUpdateLibraryPath(libraryId, pathId, data);
                update((s) => {
                    const existing = s.paths[libraryId] || [];
                    return {
                        ...s,
                        paths: {
                            ...s.paths,
                            [libraryId]: existing.map((p) =>
                                p.id === pathId ? { ...p, ...updated } : p,
                            ),
                        },
                        error: null,
                    };
                });
                return updated;
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        async removePath(libraryId, pathId) {
            try {
                await apiDeleteLibraryPath(libraryId, pathId);
                update((s) => {
                    const existing = s.paths[libraryId] || [];
                    return {
                        ...s,
                        paths: {
                            ...s.paths,
                            [libraryId]: existing.filter((p) => p.id !== pathId),
                        },
                        error: null,
                    };
                });
            } catch (err) {
                update((s) => ({ ...s, error: err }));
                throw err;
            }
        },

        isScanning(libraryId) {
            let result = false;
            const unsub = subscribe((s) => {
                result = !!s.scanning[libraryId];
            });
            unsub();
            return result;
        },

        getById(libraryId) {
            let result = null;
            const unsub = subscribe((s) => {
                result = s.items.find((lib) => lib.id === libraryId) || null;
            });
            unsub();
            return result;
        },

        clearError() {
            update((s) => ({ ...s, error: null }));
        },
    };
}

export const libraries = createLibrariesStore();

export const libraryList = derived(libraries, ($libraries) => $libraries.items);

export const currentLibrary = derived(
    libraries,
    ($libraries) => $libraries.currentLibrary,
);

export const librariesLoading = derived(libraries, ($libraries) => $libraries.loading);

export const librariesError = derived(libraries, ($libraries) => $libraries.error);
