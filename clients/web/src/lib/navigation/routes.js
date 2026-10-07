/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

const desktopPaths = [
    /^\/dashboard$/,
    /^\/libraries(?:\/[A-Za-z0-9_-]+)?$/,
    /^\/media(?:\/[A-Za-z0-9_-]+)?$/,
    /^\/play\/[A-Za-z0-9_-]+$/,
    /^\/search$/,
    /^\/collections(?:\/[A-Za-z0-9_-]+)?$/,
    /^\/settings(?:\/[A-Za-z0-9_-]+)?$/,
    /^\/auth\/link$/,
];

export function isAllowedDesktopRoute(route) {
    if (typeof route !== 'string' || !route.startsWith('/') || route.startsWith('//')) return false;
    if (/[\\\u0000-\u0020\u007f]/u.test(route)) return false;
    const url = new URL(route, 'https://duskcue.invalid');
    const path = route.split(/[?#]/u, 1)[0];
    return url.origin === 'https://duskcue.invalid'
        && url.pathname === path
        && desktopPaths.some((pattern) => pattern.test(url.pathname));
}

export function titleRoute(item, origin = null) {
    const id = item.type === 'episode' || item.type === 'season' ? item.series_id || item.id : item.id;
    const params = new URLSearchParams();
    if (item.season_id) params.set('season', item.season_id);
    if (item.type === 'episode') params.set('episode', item.id);
    if (origin && isAllowedDesktopRoute(origin)) params.set('from', origin);
    const query = params.toString();
    return `/media/${encodeURIComponent(id)}${query ? `?${query}` : ''}`;
}

export function playRoute(item, origin = null, titleDestination = null) {
    const destination = titleDestination && isAllowedDesktopRoute(titleDestination)
        ? titleDestination : titleRoute(item, origin);
    return `/play/${encodeURIComponent(item.id)}?${new URLSearchParams({ return_to: destination })}`;
}
