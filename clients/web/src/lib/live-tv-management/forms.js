/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
import { requireValue } from './schema.js';

export function sourcePair(playlist_url, guide_url) {
    for (const input of [playlist_url, guide_url]) {
        requireValue(typeof input === 'string' && input.length > 0 && new TextEncoder().encode(input).byteLength <= 4096 && !/[\s\\\u0000-\u001f\u007f]/u.test(input));
        const url = new URL(input);
        requireValue(['http:', 'https:'].includes(url.protocol) && !url.username && !url.password && !url.hash);
    }
    return { playlist_url, guide_url };
}
export function destination(expected_origin, entries) {
    requireValue(typeof expected_origin === 'string' && expected_origin.length > 0 && new TextEncoder().encode(expected_origin).byteLength <= 4096 && !/[\s\\\u0000-\u001f\u007f]/u.test(expected_origin));
    const origin = new URL(expected_origin);
    requireValue(['http:', 'https:'].includes(origin.protocol) && !origin.username && !origin.password && !origin.hash && !origin.search && origin.pathname === '/');
    const addresses = entries.split(/\r?\n/u).map((v) => v.trim()).filter(Boolean);
    requireValue(addresses.length > 0 && addresses.length <= 8 && new Set(addresses).size === addresses.length && addresses.every((v) => v.length <= 64 && /^[0-9a-fA-F:.]+$/.test(v)));
    return { expected_origin, addresses };
}
