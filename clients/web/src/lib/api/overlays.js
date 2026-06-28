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

export async function listOverlays(params = {}) {
    return get('/overlays', params);
}

export async function getOverlay(overlayId) {
    return get(`/overlays/${overlayId}`);
}

export async function createOverlay(data) {
    return post('/overlays', data);
}

export async function updateOverlay(overlayId, data) {
    return patch(`/overlays/${overlayId}`, data);
}

export async function deleteOverlay(overlayId) {
    return del(`/overlays/${overlayId}`);
}

export async function applyOverlays(data = {}) {
    return post('/overlays/apply', data);
}

export async function previewOverlay(data) {
    return post('/overlays/preview', data);
}

export async function listTemplates() {
    return get('/overlays/templates');
}

export async function importTemplate(data) {
    return post('/overlays/templates', data);
}
