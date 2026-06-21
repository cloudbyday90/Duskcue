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

import { get, post, put, del } from './core.js';

export async function listSegments(itemId, type = null) {
    const params = type ? { type } : {};
    return get(`/items/${itemId}/segments`, params);
}

export async function createSegment(itemId, data) {
    return post(`/items/${itemId}/segments`, data);
}

export async function updateSegment(itemId, segmentId, data) {
    return put(`/items/${itemId}/segments/${segmentId}`, data);
}

export async function deleteSegment(itemId, segmentId) {
    return del(`/items/${itemId}/segments/${segmentId}`);
}

export async function analyzeLibrarySegments(libraryId) {
    return post(`/libraries/${libraryId}/analyze-segments`);
}
