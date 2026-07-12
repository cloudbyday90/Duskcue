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

/**
 * Artwork URL builders for the Duskcue artwork delivery endpoint.
 *
 * The web client constructs artwork URLs from the media item ID — no
 * server-side URL embedding in media item responses. The browser sends the
 * session cookie automatically on `<img>` loads (same-origin), so the
 * authenticated endpoint works for image requests. On 404 (no artwork),
 * the `<img onerror>` handler falls back to the gradient placeholder.
 *
 * Per IMAGE_FORMATS.md, the endpoint is:
 *   GET /api/v1/items/{id}/artwork/{type}?size={size}
 *
 * Size defaults per type match the server's defaults in
 * `services/artwork_delivery.rs::default_variant_label`.
 */

export function posterUrl(itemId, size = 'w342') {
    return `/api/v1/items/${itemId}/artwork/poster?size=${size}`;
}

export function backdropUrl(itemId, size = 'w780') {
    return `/api/v1/items/${itemId}/artwork/backdrop?size=${size}`;
}

export function thumbnailUrl(itemId, size = 'w300') {
    return `/api/v1/items/${itemId}/artwork/thumbnail?size=${size}`;
}

export function logoUrl(itemId, size = 'original') {
    return `/api/v1/items/${itemId}/artwork/logo?size=${size}`;
}
