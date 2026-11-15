// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';

export const PATHS = Object.freeze(['/dot.name', '/terminal$', '/star.*', '/bracket[ab]', '/group(x)', '/plus+', '/back\\slash']);

export function routeMatches(source, path) {
    assert.equal(typeof source, 'string');
    const pattern = `${source.startsWith('^') ? '' : '^'}${source}${source.endsWith('$') ? '' : '$'}`;
    return new RegExp(pattern).test(path);
}

export function assertLiteralPatterns(config) {
    assert.equal(config.version, 3);
    assert.ok(Array.isArray(config.routes));
    const expected = [];
    for (const path of PATHS) expected.push(path);
    for (const path of PATHS) expected.push(`${path}/`, path);
    assert.equal(config.routes.length, expected.length + 2);
    for (const [index, path] of expected.entries()) {
        const source = config.routes[index].src;
        assert.ok(routeMatches(source, path), `Generated route does not match its literal path: ${index}.`);
        for (const other of [`${path}extra`, `${path}/extra`, path.replace('.', 'X'), path.replace('[ab]', 'a'), path.replace('(x)', 'x'), path.replace('$', '')]) {
            if (other !== path) assert.equal(routeMatches(source, other), false, `Generated route matches a sibling path: ${index}.`);
        }
    }
    return expected.length;
}
