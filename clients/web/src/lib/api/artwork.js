/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { get } from './core.js';

export function getArtwork(itemId, type = 'poster', size = 'w342', options = {}) {
    return get(`/items/${encodeURIComponent(itemId)}/artwork/${type}`, { size }, {
        ...options,
        responseType: 'blob',
    });
}
