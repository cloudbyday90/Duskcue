// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { join, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';

export const TRAVERSALS = Object.freeze(['/..%5Csentinel.txt', '/_app/remote/..%5C..%5C..%5Csentinel.txt']);

export function assertSentinelPaths() {
    const root = 'C:\\fixture\\prerendered';
    for (const [index, path] of TRAVERSALS.entries()) {
        const base = win32.join(root, index === 0 ? 'pages' : 'data');
        const candidate = decodeURI(win32.join(base, path));
        assert.equal(win32.normalize(candidate), win32.join(root, 'sentinel.txt'));
    }
}

export async function runPreview(inputs, fixture, owners, requests, nonce, signal) {
    assertSentinelPaths();
    const root = join(fixture.path, '.svelte-kit/output/prerendered');
    const sentinel = `outside-${nonce}`;
    await mkdir(join(root, 'data/_app/remote'), { recursive: true });
    await writeFile(join(root, 'pages/positive.txt'), `page-${nonce}`, { flag: 'wx' });
    await writeFile(join(root, 'data/_app/remote/positive.txt'), `data-${nonce}`, { flag: 'wx' });
    await writeFile(join(root, 'sentinel.txt'), sentinel, { flag: 'wx' });
    assert.equal(await readFile(join(root, 'sentinel.txt'), 'utf8'), sentinel);
    const worker = fileURLToPath(new URL('./worker.mjs', import.meta.url));
    const preview = owners.start([worker, 'preview', inputs.web, nonce], { cwd: fixture.path, nonce });
    try {
        const origin = await preview.ready();
        for (const [path, body] of [['/positive.txt', `page-${nonce}`], ['/_app/remote/positive.txt', `data-${nonce}`]]) {
            const response = await requests.get(origin, path, { signal });
            assert.equal(response.status, 200);
            assert.equal(response.body, body);
        }
        for (const path of TRAVERSALS) {
            const response = await requests.get(origin, path, { signal });
            assert.equal(response.status, 404);
            assert.ok(!response.body.includes(sentinel));
            preview.assertRunning();
        }
    } finally { await preview.stop(); }
    return { id: 'preview-windows', status: process.platform === 'win32' ? 'passed' : 'not-qualified', platform: process.platform,
        positivePages: 1, positiveData: 1, sentinelVerified: true, blockedPaths: TRAVERSALS.length };
}
