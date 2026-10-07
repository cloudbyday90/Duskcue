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

import { get, patch, del } from './core.js';

export async function listMediaItems(params = {}, options = {}) {
    return get('/media-items', params, options);
}

export function listContinueWatching(params = {}, options = {}) {
    return get('/media-items/continue-watching', params, options);
}

export function listSeriesSeasons(seriesId, params = {}, options = {}) {
    return get(`/media-items/${seriesId}/seasons`, params, options);
}

export function listSeasonEpisodes(seasonId, params = {}, options = {}) {
    return get(`/media-items/${seasonId}/episodes`, params, options);
}

export async function getMediaItem(itemId, options = {}) {
    return get(`/media-items/${itemId}`, {}, options);
}

export async function updateMediaItem(itemId, data) {
    return patch(`/media-items/${itemId}`, data);
}

export async function deleteMediaItem(itemId) {
    return del(`/media-items/${itemId}`);
}

export async function listMediaFiles(itemId, options = {}) {
    return get(`/media-items/${itemId}/files`, {}, options);
}

export async function getMediaFile(itemId, fileId) {
    return get(`/media-items/${itemId}/files/${fileId}`);
}
