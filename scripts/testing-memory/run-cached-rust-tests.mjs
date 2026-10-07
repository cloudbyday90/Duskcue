/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { access, readFile, readdir, realpath } from 'node:fs/promises';
import { isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { stopOwnedProcess } from './owned-process.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const snapshotRoots = ['server/src', 'server/tests', 'server/migrations', 'server/assets', 'crates/types/src', 'crates/db/src', 'vendor/plist-1.9.0/src'];
const snapshotFiles = ['Cargo.toml', 'Cargo.lock', 'server/Cargo.toml', 'server/build.rs', 'crates/types/Cargo.toml', 'crates/db/Cargo.toml', 'vendor/plist-1.9.0/Cargo.toml'];
const optionalSnapshotFiles = ['.cargo/config', '.cargo/config.toml', 'rust-toolchain', 'rust-toolchain.toml'];

function within(directory, target) {
    const path = relative(directory, target);
    return path !== '' && path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path);
}

export async function sha256(path) {
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(path, { highWaterMark: 65536 })) hash.update(chunk);
    return hash.digest('hex');
}

export async function sourcePaths(root) {
    const paths = [...snapshotFiles];
    async function visit(directory) {
        for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
            const path = `${directory}/${entry.name}`;
            if (entry.isDirectory()) await visit(path);
            else if (entry.isFile()) paths.push(path);
            else throw new Error(`The source snapshot contains a nonregular path: ${path}.`);
        }
    }
    for (const directory of snapshotRoots) await visit(directory);
    for (const path of optionalSnapshotFiles) {
        try { await access(join(root, path)); paths.push(path); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
    return paths.sort();
}

export async function validateSnapshot(manifest, root) {
    if (manifest.version !== 1 || manifest.kind !== 'cached-rust-lib-test'
        || !isAbsolute(manifest.executable?.path || '') || !/^[a-f0-9]{64}$/.test(manifest.executable?.sha256 || '')
        || !Array.isArray(manifest.sources) || !Array.isArray(manifest.allowedTests)
        || !manifest.allowedTests.length || manifest.allowedTests.some((name) => !/^[A-Za-z0-9_:]+$/.test(name))
        || !manifest.compilation?.memoryEvidence || !/^[a-f0-9]{64}$/.test(manifest.compilation?.memoryEvidenceSha256 || '')) {
        throw new Error('The cached Rust manifest is incomplete.');
    }
    const canonicalRoot = await realpath(root);
    const deps = await realpath(join(canonicalRoot, 'target', 'debug', 'deps'));
    const executable = await realpath(manifest.executable.path);
    if (!within(deps, executable) || !/^duskcue-[a-f0-9]+\.exe$/.test(relative(deps, executable))) {
        throw new Error('The cached executable must remain in workspace target/debug/deps.');
    }
    if (await sha256(executable) !== manifest.executable.sha256) throw new Error('The cached executable hash changed.');
    const evidence = await realpath(join(canonicalRoot, manifest.compilation.memoryEvidence));
    const evidenceDirectory = await realpath(join(canonicalRoot, '.cache', 'testing-memory'));
    if (!within(evidenceDirectory, evidence) || await sha256(evidence) !== manifest.compilation.memoryEvidenceSha256) {
        throw new Error('The recorded compile evidence changed.');
    }
    const current = await sourcePaths(canonicalRoot);
    const recorded = manifest.sources.map((row) => row.path);
    if (JSON.stringify(current) !== JSON.stringify(recorded)) throw new Error('The cached Rust source inventory changed.');
    for (const source of manifest.sources) {
        if (typeof source.path !== 'string' || isAbsolute(source.path) || !/^[a-f0-9]{64}$/.test(source.sha256 || '')) {
            throw new Error('The cached source hash record is invalid.');
        }
        const path = await realpath(join(canonicalRoot, source.path));
        if (!within(canonicalRoot, path) || await sha256(path) !== source.sha256) {
            throw new Error(`The cached Rust source changed: ${source.path}.`);
        }
    }
    return executable;
}

export function hasExactTest(output, name) {
    return output.split(/\r?\n/).includes(`${name}: test`)
        && /^1 test, 0 benchmarks\s*$/m.test(output);
}

async function run(executable, args, capture) {
    const startedAt = Date.now();
    const child = spawn(executable, args, {
        cwd: workspace,
        env: { ...process.env, RUST_TEST_THREADS: '1', CARGO_BUILD_JOBS: '2', UV_THREADPOOL_SIZE: '2' },
        windowsHide: true,
        stdio: ['ignore', 'pipe', 'pipe'],
    });
    child.startedAt = startedAt;
    let output = '';
    let bytes = 0;
    let failure;
    let stopping;
    const stop = (message) => {
        failure ||= message;
        stopping ||= stopOwnedProcess(child);
        stopping.catch(() => {});
    };
    const result = new Promise((resolveResult, reject) => {
        child.once('error', reject);
        child.once('close', (code, signal) => resolveResult({ code, signal }));
    });
    result.catch(() => {});
    const timeout = setTimeout(() => stop('The cached Rust test exceeded its deadline.'), capture ? 15000 : 300000);
    const interrupt = () => stop('The cached Rust test was interrupted.');
    process.on('SIGINT', interrupt);
    process.on('SIGTERM', interrupt);
    child.stdout.on('data', (data) => {
        bytes += data.length;
        if (bytes > (capture ? 65536 : 1048576)) { stop('The cached Rust test exceeded its output limit.'); return; }
        output = capture ? output + data.toString('utf8') : (output + data.toString('utf8')).slice(-8192);
        if (!capture) process.stdout.write(data);
    });
    child.stderr.on('data', (data) => {
        bytes += data.length;
        if (bytes > (capture ? 65536 : 1048576)) stop('The cached Rust test exceeded its output limit.');
        else process.stderr.write(data);
    });
    try {
        const outcome = await result;
        if (stopping) await stopping;
        if (failure) throw new Error(failure);
        if (outcome.code !== 0 || outcome.signal) throw new Error('The cached Rust test process failed.');
        return output;
    } finally {
        clearTimeout(timeout);
        process.off('SIGINT', interrupt);
        process.off('SIGTERM', interrupt);
        if (child.pid && child.exitCode === null && child.signalCode === null) {
            await stopOwnedProcess(child);
            await result;
        }
    }
}

async function main() {
    const [manifestPath, test, ...extra] = process.argv.slice(2);
    if (extra.length || !manifestPath || !test || process.platform !== 'win32') {
        throw new Error('Provide a cached Rust manifest and one exact test on Windows.');
    }
    const markerDirectory = await realpath(join(workspace, '.cache', 'testing-memory'));
    const resourceId = process.env.DUSKCUE_TEST_RESOURCE_ID;
    const marker = process.env.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    if (!/^[a-f0-9-]{36}$/.test(resourceId || '') || !marker
        || resolve(marker) !== join(markerDirectory, `${resourceId}.containers.jsonl`)) {
        throw new Error('Cached Rust tests require the owning memory workflow and container marker.');
    }
    const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
    if (!manifest.allowedTests.includes(test)) throw new Error('The exact test is not authorized by this artifact manifest.');
    const executable = await validateSnapshot(manifest, workspace);
    if (!hasExactTest(await run(executable, [test, '--list', '--exact'], true), test)) {
        throw new Error('The exact test is absent from the cached executable.');
    }
    await validateSnapshot(manifest, workspace);
    console.log(`Verified cached Rust artifact ${manifest.executable.sha256}; exact test ${test}.`);
    const output = await run(executable, [test, '--exact', '--include-ignored', '--nocapture'], false);
    if (!/test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;/.test(output)) {
        throw new Error('The cached Rust test did not report exactly one passing test.');
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try { await main(); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
}
