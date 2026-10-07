/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { get } from './core.js';

export function listBrowseLibraries(params = {}, options = {}) {
    return get('/browse/libraries', params, options);
}

export function getBrowseLibrary(id, options = {}) {
    return get(`/browse/libraries/${encodeURIComponent(id)}`, {}, options);
}

export function listBrowseCollections(params = {}, options = {}) {
    return get('/browse/collections', params, options);
}

export function getBrowseCollection(id, options = {}) {
    return get(`/browse/collections/${encodeURIComponent(id)}`, {}, options);
}

export function listBrowseCollectionItems(id, params = {}, options = {}) {
    return get(`/browse/collections/${encodeURIComponent(id)}/items`, params, options);
}
