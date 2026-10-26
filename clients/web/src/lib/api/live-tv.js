/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { get } from './core.js';

export function fetchCatalogPage(lane, query, signal) {
    const { channelId, ...params } = query;
    const path = lane === 'channels' ? '/live-tv/catalog/channels'
        : `/live-tv/catalog/channels/${encodeURIComponent(channelId)}/programmes${lane === 'unpositionable' ? '/unpositionable' : ''}`;
    return get(path, { ...params, limit: 20 }, { signal, cache: 'no-store', maxJsonBytes: 1_048_576 });
}
