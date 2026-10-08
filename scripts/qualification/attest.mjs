// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { hashFile } from './source.mjs';
import { assertNativeArchitecture, validateRustHost, verifyElfArchitecture } from './architecture.mjs';

export const RUNTIME_ARTIFACTS = Object.freeze({
    libTest: '/qualification/bin/duskcue-lib-tests',
    stopContract: '/qualification/bin/playback-stop-contract',
    bootstrap: '/usr/local/lib/duskcue-ffmpeg-bootstrap.so',
    probe: '/usr/local/libexec/duskcue-ffmpeg-probe',
});

export async function verifyArtifacts() {
    const manifest = JSON.parse(await readFile('/qualification/artifact-manifest.json', 'utf8'));
    if (manifest.version !== 1 || manifest.kind !== 'duskcue-linux-test-artifacts' || manifest.status !== 'passed'
        || !/^[a-f0-9]{40}$/.test(manifest.source?.commit || '') || !/^[a-f0-9]{64}$/.test(manifest.source?.sha256 || '')) throw new Error('Runtime artifact provenance is incomplete.');
    if (process.platform !== 'linux' || process.env.DUSKCUE_QUALIFICATION_ARCH && process.env.DUSKCUE_QUALIFICATION_ARCH !== manifest.architecture) throw new Error('Runtime architecture does not match its qualification manifest.');
    assertNativeArchitecture(manifest.architecture, process.arch);
    validateRustHost(manifest.toolchain?.rust || '', manifest.architecture);
    for (const [key, path] of Object.entries(RUNTIME_ARTIFACTS)) {
        if (!/^[a-f0-9]{64}$/.test(manifest.artifacts?.[key]?.sha256 || '') || await hashFile(path) !== manifest.artifacts[key].sha256) throw new Error('The runtime does not contain its exact compiled artifact.');
        await verifyElfArchitecture(path, manifest.architecture);
    }
    return manifest;
}

async function main() {
    const manifest = await verifyArtifacts();
    const managed = {};
    for (const path of ['/usr/local/libexec/duskcue-ffmpeg', '/usr/bin/ffmpeg', '/usr/bin/ffprobe', '/etc/fonts/conf.d/60-duskcue-sans-fallback.conf']) managed[path] = await hashFile(path);
    await writeFile('/qualification/runtime-provenance.json', `${JSON.stringify({ version: 1, architecture: manifest.architecture, source: manifest.source, artifacts: manifest.artifacts, managed, packages: (await readFile('/qualification/packages.txt', 'utf8')).trim().split('\n') }, null, 2)}\n`);
}

if (process.argv[1]?.endsWith('/attest.mjs')) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
