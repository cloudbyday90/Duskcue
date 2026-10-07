/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, realpath, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { hasExactTest, sha256, sourcePaths, validateSnapshot } from './run-cached-rust-tests.mjs';

async function fixture(t) {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-cached-rust-test-'));
    t.after(async () => {
        const target = await realpath(root);
        assert.equal(dirname(target), await realpath(tmpdir()));
        assert.ok(basename(target).startsWith('duskcue-cached-rust-test-'));
        await rm(target, { recursive: true, force: true });
    });
    for (const path of ['server/src', 'server/tests', 'server/migrations', 'server/assets', 'crates/types/src', 'crates/db/src', 'vendor/plist-1.9.0/src', 'target/debug/deps', '.cache/testing-memory']) {
        await mkdir(join(root, path), { recursive: true });
    }
    for (const path of ['Cargo.toml', 'Cargo.lock', 'server/Cargo.toml', 'server/build.rs', 'server/src/lib.rs', 'crates/types/Cargo.toml', 'crates/db/Cargo.toml', 'vendor/plist-1.9.0/Cargo.toml']) {
        await writeFile(join(root, path), path);
    }
    const executable = join(root, 'target/debug/deps/duskcue-abcdef.exe');
    await writeFile(executable, 'fixture executable bytes');
    const evidence = '.cache/testing-memory/compile.jsonl';
    await writeFile(join(root, evidence), 'fixture observed compile evidence');
    const sources = [];
    for (const path of await sourcePaths(root)) sources.push({ path, sha256: await sha256(join(root, path)) });
    return {
        root,
        manifest: {
            version: 1,
            kind: 'cached-rust-lib-test',
            executable: { path: executable, sha256: await sha256(executable) },
            compilation: { memoryEvidence: evidence, memoryEvidenceSha256: await sha256(join(root, evidence)) },
            allowedTests: ['module::exact'],
            sources,
        },
    };
}

test('validates an unchanged executable, compile evidence and source inventory without executing it', async (t) => {
    const { root, manifest } = await fixture(t);
    assert.equal(await validateSnapshot(manifest, root), manifest.executable.path);
});

test('refuses changed source bytes', async (t) => {
    const { root, manifest } = await fixture(t);
    await writeFile(join(root, 'server/src/lib.rs'), 'changed source');
    await assert.rejects(validateSnapshot(manifest, root), /source changed/);
});

test('refuses added source paths', async (t) => {
    const { root, manifest } = await fixture(t);
    await writeFile(join(root, 'server/src/new.rs'), 'new source');
    await assert.rejects(validateSnapshot(manifest, root), /inventory changed/);
});

test('refuses changed executable bytes and changed compile evidence', async (t) => {
    const { root, manifest } = await fixture(t);
    await writeFile(manifest.executable.path, 'different executable');
    await assert.rejects(validateSnapshot(manifest, root), /executable hash changed/);
    manifest.executable.sha256 = await sha256(manifest.executable.path);
    await writeFile(join(root, manifest.compilation.memoryEvidence), 'different evidence');
    await assert.rejects(validateSnapshot(manifest, root), /compile evidence changed/);
});

test('refuses an executable outside the workspace artifact directory', async (t) => {
    const { root, manifest } = await fixture(t);
    const foreign = join(root, 'outside.exe');
    await writeFile(foreign, 'fixture executable bytes');
    manifest.executable.path = foreign;
    await assert.rejects(validateSnapshot(manifest, root), /target\/debug\/deps/);
});

test('requires the exact listed test and exactly one test, so zero or partial names cannot pass', () => {
    assert.equal(hasExactTest('module::exact: test\n\n1 test, 0 benchmarks\n', 'module::exact'), true);
    assert.equal(hasExactTest('0 tests, 0 benchmarks\n', 'module::exact'), false);
    assert.equal(hasExactTest('module::exact_extra: test\n\n1 test, 0 benchmarks\n', 'module::exact'), false);
    assert.equal(hasExactTest('module::exact: test\n\n2 tests, 0 benchmarks\n', 'module::exact'), false);
});
