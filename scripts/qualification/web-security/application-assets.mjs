// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';

export function applicationEntryPaths(html, origin) {
    const entries = [];
    const kinds = new Set();
    for (const match of html.matchAll(/\bimport\(\s*["']([^"']+)["']\s*\)/g)) {
        const url = new URL(match[1], `${origin}/auth/login`);
        const kind = /^\/_app\/immutable\/entry\/(start|app)\.[A-Za-z0-9_-]+\.js$/.exec(url.pathname)?.[1];
        if (!kind) continue;
        assert.equal(url.origin, origin, 'Application entry import must remain on the owned origin.');
        assert.ok(!url.search && !url.hash && !kinds.has(kind), 'Unexpected or duplicate application entry reference.');
        kinds.add(kind); entries.push(url.pathname);
    }
    assert.equal(entries.length, 2, 'SSR shell must reference both application entry assets.');
    return entries;
}
