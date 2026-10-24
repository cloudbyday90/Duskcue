// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { validateTransportProof } from './live-transport-proof.mjs';

const proof = { version: 1, resource_id: 'owned-fixture', rotated_sequence: 30, peak_cache_bytes: 1000, peak_cache_files: 19, decoded: [[691200, 997], [691200, 997]], stop_ms: 100, redirect_requests: 0, catalog_sessions: 0, cookie_and_bearer: true, scope_refusals: true, expired_refusal: true, worker_cleanup: true, upstream_cleanup: true, cache_cleanup: true, blocked_pipe_cancel: true, budget_refusal: true, shutdown_joined: true, ambient_proxy_ignored: true, owner_drop_joined: true, file_budget_refusal: true, eof_refusal: true };
const output = (value) => `LIVE_TRANSPORT_RESULT ${JSON.stringify(value)}\ntest result: ok. 1 passed; 0 failed; 0 ignored;\n`;

test('one owned decoded, authenticated, rotated and cleaned execution qualifies', () => {
    assert.equal(validateTransportProof(output(proof), 'owned-fixture').rotated_sequence, 30);
});

test('zero tests, duplicates, another owner and missing cleanup cannot qualify', () => {
    for (const text of ['', output(proof).replace('1 passed', '0 passed'), output(proof) + output(proof), output({ ...proof, resource_id: 'other' }), output({ ...proof, shutdown_joined: false })]) {
        assert.throws(() => validateTransportProof(text, 'owned-fixture'));
    }
});

test('metadata without real media, complete rotation or bounded cache is refused', () => {
    for (const patch of [{ decoded: [] }, { decoded: [[691200, 997], [691200, 0]] }, { rotated_sequence: 29 }, { peak_cache_bytes: 33554433 }, { peak_cache_files: 25 }, { stop_ms: 10000 }]) {
        assert.throws(() => validateTransportProof(output({ ...proof, ...patch }), 'owned-fixture'));
    }
});
