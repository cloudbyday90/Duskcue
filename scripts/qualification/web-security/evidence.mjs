// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { FETCH_CASES, validateFetchRows } from './self-fetch.mjs';

export const MATRIX_IDS = Object.freeze(['reroute', 'static-patterns', 'cache-attribute', 'cache-browser', 'client-origin', 'preview-windows', ...FETCH_CASES.map((item) => item.id)]);

export function validateMatrix(rows, nonce, platform = process.platform) {
    assert.equal(rows.length, MATRIX_IDS.length, 'Incomplete synthetic matrix.');
    assert.deepEqual([...new Set(rows.map((row) => row.id))].sort(), [...MATRIX_IDS].sort(), 'Missing, duplicate or unknown boundary case.');
    const row = (id) => rows.find((item) => item.id === id);
    for (const item of rows) assert.equal(item.status, item.id === 'preview-windows' && platform !== 'win32' ? 'not-qualified' : 'passed');
    assert.equal(row('reroute').malformedPaths, 4); assert.equal(row('reroute').healthyRequests, 7);
    assert.equal(row('static-patterns').literalPatterns, 21); assert.equal(row('cache-attribute').artifactChecked, true);
    const browser = row('cache-browser');
    assert.equal(browser.inlineControl, 1); assert.equal(browser.markerAbsent, true); assert.equal(browser.dialogs, 0); assert.equal(browser.cleanupConfirmed, true);
    const origin = row('client-origin');
    assert.equal(origin.hydrated, true); assert.ok(origin.healthyRouteRequests >= 1); assert.ok(origin.hostileRouteRequests >= 1);
    assert.equal(origin.foreignRequests, 0); assert.equal(origin.markerAbsent, true); assert.equal(origin.cleanupConfirmed, true);
    const preview = row('preview-windows');
    assert.equal(preview.platform, platform); assert.equal(preview.positivePages, 1); assert.equal(preview.positiveData, 1);
    assert.equal(preview.sentinelVerified, true); assert.equal(preview.blockedPaths, 2);
    const fetched = FETCH_CASES.map((item) => row(item.id));
    for (const item of fetched) assert.equal(item.sinkCleanupConfirmed, true);
    validateFetchRows(fetched, nonce);
    return { boundaryMatrixPassed: platform === 'win32', completeSecurityMatrix: false, cases: rows };
}
