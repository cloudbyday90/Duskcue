// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { readFile, lstat, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import { createHash } from 'node:crypto';

export const EXPECTED = Object.freeze({ '@sveltejs/kit': '2.70.4', '@sveltejs/adapter-static': '3.0.11', '@sveltejs/adapter-node': '5.5.7' });
export const SUPPORT = Object.freeze(['vite', 'svelte', '@sveltejs/vite-plugin-svelte', '@playwright/test', 'playwright', 'playwright-core']);

export async function readJson(path, limit = 65536, signal) {
    const info = await lstat(path);
    assert.ok(info.isFile() && !info.isSymbolicLink() && info.size <= limit, 'Expected a bounded regular JSON file.');
    const bytes = await readFile(path, { signal });
    assert.ok(bytes.length <= limit, 'JSON input exceeded its bound.');
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
}

export function validateInstallation(manifest, lock, installed) {
    assert.equal(lock.lockfileVersion, 3);
    for (const group of ['dependencies', 'devDependencies']) assert.deepEqual(lock.packages[''][group], manifest[group]);
    for (const [name, version] of Object.entries(EXPECTED)) assert.equal(lock.packages[`node_modules/${name}`]?.version, version, `Unexpected locked ${name}.`);
    for (const name of [...Object.keys(EXPECTED), ...SUPPORT]) {
        const version = lock.packages[`node_modules/${name}`]?.version;
        assert.ok(version, `Missing locked ${name}.`);
        assert.equal(installed[name]?.name, name, `Unexpected installed identity: ${name}.`);
        assert.equal(installed[name]?.version, version, `Install the locked graph before qualifying ${name}.`);
    }
    return Object.fromEntries(Object.entries(installed).map(([name, value]) => [name, value.version]));
}

export async function installedInputs(web, signal) {
    const root = await realpath(web);
    const manifest = await readJson(join(root, 'package.json'), 65536, signal);
    const lockPath = join(root, 'package-lock.json');
    const lock = await readJson(lockPath, 2097152, signal);
    const installed = {};
    for (const name of [...Object.keys(EXPECTED), ...SUPPORT]) installed[name] = await readJson(join(root, 'node_modules', name, 'package.json'), 65536, signal);
    const versions = validateInstallation(manifest, lock, installed);
    return { web: root, versions, lockSha256: createHash('sha256').update(await readFile(lockPath, { signal })).digest('hex') };
}
