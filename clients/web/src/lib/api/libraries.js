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

import { get, post, patch, del } from './core.js';

export async function listLibraries(params = {}) {
    return get('/libraries', params);
}

export async function getLibrary(libraryId) {
    return get(`/libraries/${libraryId}`);
}

export async function createLibrary(data) {
    return post('/libraries', data);
}

export async function updateLibrary(libraryId, data) {
    return patch(`/libraries/${libraryId}`, data);
}

export async function deleteLibrary(libraryId) {
    return del(`/libraries/${libraryId}`);
}

export async function scanLibrary(libraryId, params = {}) {
    return post(`/libraries/${libraryId}/scan`, undefined, { params });
}

export async function listLibraryItems(libraryId, params = {}) {
    return get(`/libraries/${libraryId}/items`, params);
}

export async function listLibraryPaths(libraryId, params = {}) {
    return get(`/libraries/${libraryId}/paths`, params);
}

export async function getLibraryPath(libraryId, pathId) {
    return get(`/libraries/${libraryId}/paths/${pathId}`);
}

export async function createLibraryPath(libraryId, data) {
    return post(`/libraries/${libraryId}/paths`, data);
}

export async function updateLibraryPath(libraryId, pathId, data) {
    return patch(`/libraries/${libraryId}/paths/${pathId}`, data);
}

export async function deleteLibraryPath(libraryId, pathId) {
    return del(`/libraries/${libraryId}/paths/${pathId}`);
}

