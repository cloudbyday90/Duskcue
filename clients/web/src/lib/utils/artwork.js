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
