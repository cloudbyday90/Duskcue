// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { lstat, readdir } from 'node:fs/promises';
import { join } from 'node:path';

export const SOURCE_ROOTS = Object.freeze([
    'server/src', 'server/tests', 'server/migrations', 'server/assets', 'server/locales',
    'crates/types/src', 'crates/db/src', 'vendor/plist-1.9.0/src', 'clients/desktop/src-tauri/src',
    'native/ffmpeg-bootstrap', 'docker/fonts', 'docker/qualification', 'scripts/qualification',
]);
export const SOURCE_FILES = Object.freeze([
    'Cargo.toml', 'Cargo.lock', 'Dockerfile', 'LICENSE', 'server/Cargo.toml', 'server/build.rs', 'server/sqlx.toml',
    'crates/types/Cargo.toml', 'crates/db/Cargo.toml', 'vendor/plist-1.9.0/Cargo.toml',
    'clients/desktop/src-tauri/Cargo.toml', 'clients/desktop/src-tauri/build.rs',
    'docs/ci/FFMPEG_LAUNCH_READINESS.md', 'docs/ci/FFMPEG_LAUNCHER_QUALIFICATION.md', 'docs/design/PLAYBACK_CAPTION_FONTS.md',
    '.github/workflows/tonight-qualification.yml',
]);
export const OPTIONAL_SOURCE_FILES = Object.freeze([
    '.cargo/config', '.cargo/config.toml', 'rust-toolchain', 'rust-toolchain.toml',
    'crates/types/build.rs', 'crates/db/build.rs', 'vendor/plist-1.9.0/build.rs',
]);

export async function hashFile(path) {
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(path, { highWaterMark: 65536 })) hash.update(chunk);
    return hash.digest('hex');
}

export async function qualificationSources(root) {
    const paths = [...SOURCE_FILES];
    async function visit(directory) {
        const metadata = await lstat(join(root, directory));
        if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error(`Source directory is not regular: ${directory}.`);
        for (const entry of await readdir(join(root, directory), { withFileTypes: true })) {
            const path = `${directory}/${entry.name}`;
            if (entry.isDirectory()) await visit(path);
            else if (entry.isFile()) paths.push(path);
            else throw new Error(`Source inventory contains a nonregular path: ${path}.`);
        }
    }
    for (const directory of SOURCE_ROOTS) await visit(directory);
    for (const path of OPTIONAL_SOURCE_FILES) {
        try { await lstat(join(root, path)); paths.push(path); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
    }
    const files = [];
    for (const path of paths.sort()) {
        const metadata = await lstat(join(root, path));
        if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 268435456) throw new Error(`Source input is not a bounded regular file: ${path}.`);
        files.push({ path, sha256: await hashFile(join(root, path)) });
    }
    return { files, sha256: createHash('sha256').update(JSON.stringify(files), 'utf8').digest('hex') };
}
