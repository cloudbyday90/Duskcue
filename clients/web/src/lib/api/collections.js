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

import { get, post, patch, put, del } from './core.js';

export async function listCollections(params = {}) {
    return get('/collections', params);
}

export async function getCollection(collectionId) {
    return get(`/collections/${collectionId}`);
}

export async function createCollection(data) {
    return post('/collections', data);
}

export async function updateCollection(collectionId, data) {
    return patch(`/collections/${collectionId}`, data);
}

export async function deleteCollection(collectionId) {
    return del(`/collections/${collectionId}`);
}

export async function listCollectionItems(collectionId, params = {}) {
    return get(`/collections/${collectionId}/items`, params);
}

export async function addCollectionItems(collectionId, data) {
    return post(`/collections/${collectionId}/items`, data);
}

export async function reorderCollectionItems(collectionId, data) {
    return put(`/collections/${collectionId}/items/reorder`, data);
}

export async function removeCollectionItem(collectionId, mediaItemId) {
    return del(`/collections/${collectionId}/items/${mediaItemId}`);
}

export async function syncAllCollections(data = {}) {
    return post('/collections/sync', data);
}

export async function syncCollection(collectionId, data = {}) {
    return post(`/collections/${collectionId}/sync`, data);
}

export async function listTemplates() {
    return get('/collections/templates');
}

export async function importTemplate(data) {
    return post('/collections/templates', data);
}
