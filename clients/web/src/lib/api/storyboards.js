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

import { get, post, del } from './core.js';

function mediaFileParams(mediaFileId) {
    return mediaFileId ? { media_file_id: mediaFileId } : {};
}

export async function getStoryboard(itemId, mediaFileId = null) {
    return get(`/items/${itemId}/storyboard`, mediaFileParams(mediaFileId));
}

export async function getStoryboardIndex(itemId, mediaFileId = null, options = {}) {
    return get(`/items/${itemId}/storyboard/index.vtt`, mediaFileParams(mediaFileId), {
        ...options,
        headers: {
            Accept: 'text/vtt',
            ...options.headers,
        },
        responseType: 'text',
    });
}

export async function getStoryboardSprite(itemId, spriteName, mediaFileId = null, options = {}) {
    return get(`/items/${itemId}/storyboard/${encodeURIComponent(spriteName)}`, mediaFileParams(mediaFileId), {
        ...options,
        headers: {
            Accept: 'image/webp',
            ...options.headers,
        },
        responseType: 'blob',
    });
}

export async function generateLibraryStoryboards(libraryId) {
    return post(`/libraries/${libraryId}/generate-storyboards`);
}

export async function generateItemStoryboards(itemId) {
    return post(`/items/${itemId}/generate-storyboards`);
}

export async function deleteStoryboard(itemId) {
    return del(`/items/${itemId}/storyboard`);
}
