// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';

export function assertSseResponse(response, nonce) {
    assert.equal(response.status, 200);
    assert.equal(response.headers['content-type'], 'text/event-stream');
    assert.equal(response.headers['x-accel-buffering'], 'no');
    assert.equal(response.body, `data: ${nonce}\n\n`);
}

export async function runSseCases(origin, requests, server, nonce, signal) {
    for (const path of ['/immutable-sse', '/mutable-sse']) {
        assertSseResponse(await requests.get(origin, path, { signal }), nonce);
        const healthy = await requests.get(origin, '/healthy', { signal });
        assert.equal(healthy.status, 200);
        assert.equal(healthy.body, `healthy-${nonce}`);
        assert.equal(healthy.headers['x-accel-buffering'], undefined);
        server.assertRunning();
    }
    return { id: 'immutable-sse', status: 'passed', immutable: true, mutable: true, healthyRequests: 2, ordinaryHeaderAbsent: true };
}
