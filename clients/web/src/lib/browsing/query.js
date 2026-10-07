/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { isAllowedDesktopRoute } from '../navigation/routes.js';

const types = new Set(['movie', 'series', 'season', 'episode']);
const catalogSorts = new Set(['added', 'title', 'year']);
const watchStates = new Set(['watched', 'unwatched', 'in_progress']);

export function paginationDepth(value) {
    const number = Number(value);
    return /^\d+$/.test(String(value)) && Number.isSafeInteger(number) && number > 0 ? number : 1;
}

export function catalogQuery(searchParams, scope = {}) {
    const sort = catalogSorts.has(searchParams.get('sort')) ? searchParams.get('sort') : 'added';
    const order = ['asc', 'desc'].includes(searchParams.get('order')) ? searchParams.get('order') : sort === 'title' ? 'asc' : 'desc';
    const type = types.has(searchParams.get('type')) ? searchParams.get('type') : undefined;
    const watch = watchStates.has(searchParams.get('watch')) ? searchParams.get('watch') : 'all';
    const favorite = searchParams.get('favorite');
    const libraryId = scope.libraryId || searchParams.get('library_id') || undefined;
    return {
        ...(libraryId ? { library_id: libraryId } : {}),
        ...(type ? { type } : {}),
        sort,
        order,
        watch,
        ...(favorite === 'true' || favorite === 'false' ? { favorite: favorite === 'true' } : {}),
    };
}

export function searchQuery(searchParams) {
    const sort = ['relevance', 'title', 'year'].includes(searchParams.get('sort')) ? searchParams.get('sort') : 'relevance';
    const order = ['asc', 'desc'].includes(searchParams.get('order')) ? searchParams.get('order') : sort === 'title' ? 'asc' : 'desc';
    const favorite = searchParams.get('favorite');
    return {
        q: (searchParams.get('q') || '').trim(),
        ...(types.has(searchParams.get('type')) ? { type: searchParams.get('type') } : {}),
        ...(searchParams.get('genre') ? { genre: searchParams.get('genre') } : {}),
        ...(searchParams.get('year') ? { year: searchParams.get('year') } : {}),
        ...(searchParams.get('rating_min') ? { rating_min: searchParams.get('rating_min') } : {}),
        ...(watchStates.has(searchParams.get('watch')) ? { watch: searchParams.get('watch') } : {}),
        ...(favorite === 'true' || favorite === 'false' ? { favorite: favorite === 'true' } : {}),
        sort,
        order,
    };
}

export function browsingUrl(url, changes, resetPages = true) {
    const next = new URL(url);
    for (const [key, value] of Object.entries(changes)) {
        if (value === '' || value === null || value === undefined) next.searchParams.delete(key);
        else next.searchParams.set(key, String(value));
    }
    if (resetPages) next.searchParams.delete('pages');
    next.searchParams.delete('cursor');
    const query = next.searchParams.toString();
    return `${next.pathname}${query ? `?${query}` : ''}`;
}

export function collectionQuery(searchParams) {
    const query = catalogQuery(searchParams);
    delete query.library_id;
    if (!catalogSorts.has(searchParams.get('sort'))) {
        delete query.sort;
        query.order = 'asc';
    }
    return query;
}

export function currentOrigin(url) {
    return `${url.pathname}${url.search}`;
}

export function collectionListOrigin(value) {
    return listOrigin(value, '/collections');
}

export function libraryListOrigin(value) {
    return listOrigin(value, '/libraries');
}

function listOrigin(value, path) {
    if (!isAllowedDesktopRoute(value) || new URL(value, 'https://duskcue.invalid').pathname !== path) return path;
    return value;
}

export function collectionBrowseRoute(id, origin) {
    return `/collections/${encodeURIComponent(id)}?${new URLSearchParams({ from: collectionListOrigin(origin) })}`;
}

export function libraryBrowseRoute(id, origin) {
    return `/libraries/${encodeURIComponent(id)}?${new URLSearchParams({ from: libraryListOrigin(origin) })}`;
}
