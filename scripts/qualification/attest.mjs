// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { hashFile } from './source.mjs';

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
    for (const [key, path] of Object.entries(RUNTIME_ARTIFACTS)) {
        if (!/^[a-f0-9]{64}$/.test(manifest.artifacts?.[key]?.sha256 || '') || await hashFile(path) !== manifest.artifacts[key].sha256) throw new Error('The runtime does not contain its exact compiled artifact.');
    }
    return manifest;
}

async function main() {
    const manifest = await verifyArtifacts();
    const managed = {};
    for (const path of ['/usr/local/libexec/duskcue-ffmpeg', '/usr/bin/ffmpeg', '/usr/bin/ffprobe', '/etc/fonts/conf.d/60-duskcue-sans-fallback.conf']) managed[path] = await hashFile(path);
    await writeFile('/qualification/runtime-provenance.json', `${JSON.stringify({ version: 1, source: manifest.source, artifacts: manifest.artifacts, managed, packages: (await readFile('/qualification/packages.txt', 'utf8')).trim().split('\n') }, null, 2)}\n`);
}

if (process.argv[1]?.endsWith('/attest.mjs')) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
