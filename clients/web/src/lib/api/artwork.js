import { get } from './core.js';

export function getArtwork(itemId, type = 'poster', size = 'w342', options = {}) {
    return get(`/items/${encodeURIComponent(itemId)}/artwork/${type}`, { size }, {
        ...options,
        responseType: 'blob',
    });
}
