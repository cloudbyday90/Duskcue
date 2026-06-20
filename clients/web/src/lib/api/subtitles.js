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

import { get, post, put, del, buildApiUrl } from './core.js';

export async function getSubtitleSettings() {
    return get('/settings/subtitles');
}

export async function updateSubtitleSettings(data) {
    return put('/settings/subtitles', data);
}

export async function updateSubtitleProviderSettings(data) {
    return put('/settings/subtitles/providers', data);
}

export async function listSubtitles(itemId) {
    return get(`/items/${itemId}/subtitles`);
}

export async function getSubtitle(itemId, subtitleId) {
    return get(`/items/${itemId}/subtitles/${subtitleId}`);
}

export function getSubtitleContentUrl(itemId, subtitleId, format = null) {
    return buildApiUrl(`/items/${itemId}/subtitles/${subtitleId}/content`, format ? { format } : {});
}

export async function fetchSubtitles(itemId, data) {
    return post(`/items/${itemId}/subtitles`, data);
}

export async function setSubtitleOffset(itemId, subtitleId, offsetMs) {
    return put(`/items/${itemId}/subtitles/${subtitleId}/offset`, { offset_ms: offsetMs });
}

export async function triggerOcr(itemId, subtitleId, engine = null) {
    return post(`/items/${itemId}/subtitles/${subtitleId}/ocr`, engine ? { engine } : {});
}

export async function getSubtitleSyncData(itemId, subtitleId) {
    return get(`/items/${itemId}/subtitles/${subtitleId}/sync`);
}

export async function deleteSubtitle(itemId, subtitleId) {
    return del(`/items/${itemId}/subtitles/${subtitleId}`);
}
