// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { copyFile, lstat, mkdir, readdir, readFile, realpath } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

const roots = ['server/src', 'server/tests', 'server/assets', 'server/migrations', 'server/locales', 'crates/types/src', 'crates/db/src', 'vendor/plist-1.9.0/src', 'clients/desktop/src-tauri/src'];
const requiredFiles = ['Cargo.toml', 'Cargo.lock', 'server/Cargo.toml', 'server/build.rs', 'server/sqlx.toml', 'crates/types/Cargo.toml', 'crates/db/Cargo.toml', 'vendor/plist-1.9.0/Cargo.toml', 'clients/desktop/src-tauri/Cargo.toml', 'clients/desktop/src-tauri/build.rs'];
const optionalFiles = ['.cargo/config', '.cargo/config.toml', 'rust-toolchain', 'rust-toolchain.toml', 'crates/types/build.rs', 'crates/db/build.rs', 'vendor/plist-1.9.0/build.rs'];

export async function fileHash(path) {
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(path, { highWaterMark: 65536 })) hash.update(chunk);
    return hash.digest('hex');
}

export async function compilerSourceManifest(root) {
    const paths = [...requiredFiles];
    async function visit(directory) {
        for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
            const path = `${directory}/${entry.name}`;
            if (entry.isDirectory()) await visit(path);
            else if (entry.isFile()) paths.push(path);
            else throw new Error(`Compiler source contains a nonregular path: ${path}.`);
        }
    }
    for (const directory of roots) await visit(directory);
    for (const path of optionalFiles) {
        try { await lstat(join(root, path)); paths.push(path); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
    const sources = [];
    for (const path of paths.sort()) {
        if (!(await lstat(join(root, path))).isFile()) throw new Error(`Compiler source must be regular: ${path}.`);
        sources.push({ path, sha256: await fileHash(join(root, path)) });
    }
    return { sources, sha256: createHash('sha256').update(JSON.stringify(sources)).digest('hex') };
}

export async function copyCompilerSnapshot(root, snapshot, expected) {
    for (const directory of roots) await mkdir(join(snapshot, directory), { recursive: true });
    for (const row of expected.sources) {
        const destination = join(snapshot, row.path);
        await mkdir(dirname(destination), { recursive: true });
        await copyFile(join(root, row.path), destination);
    }
    if ((await compilerSourceManifest(snapshot)).sha256 !== expected.sha256) throw new Error('Compiler source changed while its isolated snapshot was copied.');
}

export async function validateCompilerRegistry(directory) {
    const canonical = await realpath(directory);
    if ([',', '\n', '\r'].some((character) => canonical.includes(character))) throw new Error('The offline registry path cannot be encoded as a Docker bind mount.');
    for (const name of ['cache', 'index', 'src']) {
        const path = join(canonical, name);
        if (!(await lstat(path)).isDirectory() || !(await readdir(path)).length) throw new Error(`A prepared offline Cargo registry requires a nonempty ${name} directory.`);
    }
    return canonical;
}

export async function owningCompilerDirectory(workspace, environment) {
    const resourceId = environment.DUSKCUE_TEST_RESOURCE_ID;
    const marker = environment.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    const root = await realpath(join(workspace, '.cache', 'testing-memory'));
    if (!/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(resourceId || '') || !marker || resolve(marker) !== join(root, `${resourceId}.containers.jsonl`)) throw new Error('Linux compilation requires the owning normal Cargo memory workflow.');
    const deadline = Date.now() + 1000;
    let active;
    do {
        active = JSON.parse(await readFile(join(root, 'active.json'), 'utf8'));
        if (active.pid !== process.ppid || active.workload !== 'linux-cargo-check') throw new Error('The compiler is not the child of its owning normal Cargo workflow.');
        if (active.childPid === process.pid) break;
        if (active.childPid !== null) throw new Error('The Cargo workflow owns a different child.');
        await new Promise((resolveWait) => setTimeout(resolveWait, 20));
    } while (Date.now() < deadline);
    if (active.childPid !== process.pid) throw new Error('The compiler child was not registered in the owning Cargo workflow.');
    const directory = join(root, `linux-cargo-${resourceId}`);
    await mkdir(directory, { recursive: false });
    const canonical = await realpath(directory);
    const inside = relative(root, canonical);
    if (!inside || inside.startsWith(`..${sep}`) || inside === '..' || isAbsolute(inside)) throw new Error('Compiler evidence is outside the owning workspace.');
    return { resourceId, marker, directory: canonical };
}

export function validateCompilerImage(image, expected, architecture) {
    const normalizedArchitecture = architecture === 'x86_64' ? 'amd64' : architecture === 'aarch64' ? 'arm64' : architecture;
    const toolchain = image?.Config?.Labels?.['duskcue.qualification.rust-toolchain'];
    if (!/^sha256:[a-f0-9]{64}$/.test(expected || '') || image?.Id !== expected || image?.Os !== 'linux' || image?.Architecture !== normalizedArchitecture
        || image?.Config?.Labels?.['duskcue.qualification.compiler'] !== 'linux-cargo-check-v1' || !/^\d+\.\d+\.\d+$/.test(toolchain || '')) throw new Error('A local immutable prepared Linux compiler image with explicit toolchain provenance is required.');
    return { id: image.Id, architecture: image.Architecture, toolchain };
}

export function compilerContainerArgs({ context, image, toolchain, resourceId, name, snapshot, artifacts, registry }) {
    if (!/^[A-Za-z0-9_.-]+$/.test(context || '') || !/^sha256:[a-f0-9]{64}$/.test(image || '') || !/^\d+\.\d+\.\d+$/.test(toolchain || '')
        || !/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/.test(resourceId || '') || name !== `duskcue-playback-${resourceId}`
        || [snapshot, artifacts, registry].some((path) => !isAbsolute(path || '') || /[,\r\n]/.test(path))) throw new Error('The fixed compiler container inputs are invalid.');
    return ['--context', context, 'run', '--pull=never', '--name', name, '--label', `duskcue.test.resource-id=${resourceId}`,
        '--memory=4g', '--memory-swap=4g', '--cpus=2', '--pids-limit=256', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges', '--user=65532:65532',
        '--tmpfs', '/tmp:rw,nosuid,nodev,size=268435456', '--mount', `type=bind,source=${snapshot},target=/source,readonly`,
        '--mount', `type=bind,source=${artifacts},target=/qualification`, '--mount', `type=bind,source=${registry},target=/qualification/cargo/registry,readonly`,
        '--workdir=/source', '--env=CARGO_HOME=/qualification/cargo', '--env=CARGO_TARGET_DIR=/qualification/target', '--env=HOME=/qualification/home',
        `--env=RUSTUP_TOOLCHAIN=${toolchain}`, '--env=CARGO_BUILD_JOBS=2', '--env=RUST_TEST_THREADS=1', '--env=SQLX_OFFLINE=true',
        '--entrypoint=/bin/sh', image, '-c', 'set -eu; rustc -vV; cargo --version; exec cargo check -p duskcue --tests --locked --offline -j 2'];
}
