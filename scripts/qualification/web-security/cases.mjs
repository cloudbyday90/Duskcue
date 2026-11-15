// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { join } from 'node:path';
import { readFile, lstat } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { fixtureEnvironment } from './process.mjs';
import { readJson } from './prerequisite.mjs';
import { assertLiteralPatterns } from './patterns.mjs';
import { CACHE_VALUE } from './fixture.mjs';
import { runBrowserCases } from './browser-cases.mjs';
import { runPreview } from './preview.mjs';
import { validateFetchRows } from './self-fetch.mjs';
import { validateMatrix } from './evidence.mjs';
import { runSseCases } from './sse.mjs';

const worker = fileURLToPath(new URL('./worker.mjs', import.meta.url));

export function assertCacheAttribute(html) {
    assert.ok(html.includes('<h1>Cache fixture</h1>'), 'Missing ordinary prerendered content.');
    const matches = [...html.matchAll(/<meta http-equiv="cache-control" content="([^"]*)">/g)];
    assert.equal(matches.length, 1, 'Expected one intact cache attribute.');
    const decoded = matches[0][1].replace(/&quot;/g, '"').replace(/&amp;/g, '&');
    assert.equal(decoded, CACHE_VALUE, 'Cache attribute did not preserve escaped evidence.');
}

export async function runCases(inputs, fixture, owners, requests, nonce, signal, browser, fail) {
    const staticWork = owners.start([worker, 'static', inputs.web, nonce], { cwd: fixture.path });
    await staticWork.wait(15000);
    const config = await readJson(join(fixture.path, '.vercel/output/config.json'), 65536, signal);
    const literalPatterns = assertLiteralPatterns(config);
    const server = owners.start([worker, 'server', inputs.web, nonce], { cwd: fixture.path, nonce,
        env: fixtureEnvironment({ HOST: '127.0.0.1', PORT: '0', SHUTDOWN_TIMEOUT: '1' }) });
    const rows = [];
    try {
        const origin = await server.ready();
        const healthy = async (path = '/healthy') => {
            const response = await requests.get(origin, path, { signal });
            assert.equal(response.status, 200);
            assert.equal(response.body, `healthy-${nonce}`);
            server.assertRunning();
        };
        await healthy();
        for (const path of ['/bad/%', '/bad/%FF', '/bad/%E0%A4%A', '/bad/%c0%ae']) {
            const response = await requests.get(origin, path, { signal });
            assert.equal(response.status, 400, 'Malformed original pathname must return a production 400.');
            await healthy();
        }
        await healthy('/alias');
        await healthy('/fr/healthy');
        rows.push(await runSseCases(origin, requests, server, nonce, signal));
        const artifact = join(fixture.path, '.svelte-kit/output/prerendered/pages/cache.html');
        assert.ok((await lstat(artifact)).size <= 1048576, 'Prerendered fixture exceeded its bound.');
        assertCacheAttribute(await readFile(artifact, { encoding: 'utf8', signal }));
        rows.push({ id: 'reroute', status: 'passed', malformedPaths: 4, healthyRequests: 7 },
            { id: 'static-patterns', status: 'passed', literalPatterns }, { id: 'cache-attribute', status: 'passed', artifactChecked: true });
        const response = await requests.get(origin, '/self-fetch', { signal });
        assert.equal(response.status, 200);
        const fetched = JSON.parse(response.body);
        assert.equal(fetched.nonce, nonce);
        assert.equal(fetched.sinkCleanupConfirmed, true);
        validateFetchRows(fetched.rows, nonce);
        rows.push(...fetched.rows.map((row) => ({ ...row, status: 'passed', sinkCleanupConfirmed: true })));
        rows.push(...await runBrowserCases(inputs, origin, browser, owners, signal, fail));
    } finally { await server.stop(); }
    rows.push(await runPreview(inputs, fixture, owners, requests, nonce, signal));
    owners.assertHealthy();
    return { reroute: true, malformedPaths: 4, healthyRequests: 7, staticPatterns: true, literalPatterns, cacheAttribute: true, ...validateMatrix(rows, nonce) };
}
