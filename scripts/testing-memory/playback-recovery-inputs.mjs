// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { copyFile, lstat, mkdir, readdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { hashFile, qualificationSources } from '../qualification/source.mjs';

export function parseRecoveryArguments(args) {
    if (args.length !== 2 || args[0] !== '--image' || !/^sha256:[a-f0-9]{64}$/.test(args[1])) throw new Error('Provide --image <immutable final local Duskcue image SHA256>.');
    return { image: args[1] };
}

export async function owningRecoveryDirectory(workspace, environment) {
    const root = await realpath(join(workspace, '.cache/testing-memory'));
    const resourceId = environment.DUSKCUE_TEST_RESOURCE_ID;
    const marker = environment.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    if (!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(resourceId || '') || !marker || resolve(marker) !== join(root, `${resourceId}.containers.jsonl`)) throw new Error('Recovery qualification requires its owning build memory workflow.');
    const deadline = Date.now() + 1000;
    let active;
    do {
        active = JSON.parse(await readFile(join(root, 'active.json'), 'utf8'));
        if (active.pid !== process.ppid || active.workload !== 'playback-recovery-qualify') throw new Error('Recovery qualification must be the child of its owning build workflow.');
        if (active.childPid === process.pid) break;
        if (active.childPid !== null) throw new Error('The build workflow owns a different child.');
        await new Promise((resume) => setTimeout(resume, 20));
    } while (Date.now() < deadline);
    if (active.childPid !== process.pid) throw new Error('The recovery child was not registered in its owning build workflow.');
    const directory = join(root, `playback-recovery-${resourceId}`);
    await mkdir(directory);
    if (dirname(await realpath(directory)) !== root || (await lstat(directory)).isSymbolicLink()) throw new Error('Recovery evidence escaped its owning workspace.');
    return { resourceId, marker, directory };
}

export async function recoverySources(workspace) {
    const files = (await qualificationSources(workspace)).files;
    async function visit(directory) {
        const info = await lstat(join(workspace, directory));
        if (!info.isDirectory() || info.isSymbolicLink()) throw new Error('Recovery driver directory is not regular.');
        for (const entry of await readdir(join(workspace, directory), { withFileTypes: true })) {
            const path = `${directory}/${entry.name}`;
            if (entry.isDirectory()) await visit(path);
            else if (entry.isFile()) {
                if ((await lstat(join(workspace, path))).size > 1048576) throw new Error('Recovery driver source exceeds its file bound.');
                files.push({ path, sha256: await hashFile(join(workspace, path)) });
            } else throw new Error('Recovery driver source is not a regular file.');
        }
    }
    await visit('scripts/testing-memory');
    files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}

export async function copyRecoverySnapshot(workspace, directory, expected) {
    await mkdir(directory);
    for (const file of expected.files) {
        const destination = join(directory, file.path);
        await mkdir(dirname(destination), { recursive: true });
        await copyFile(join(workspace, file.path), destination);
    }
    await writeFile(join(directory, '.dockerignore'), '.git\nnode_modules\n**/node_modules\ntarget\n**/target\n.env\n.env.*\n');
    if ((await recoverySources(directory)).sha256 !== expected.sha256) throw new Error('Recovery sources changed while copying the current working tree.');
}

export function validateRecoveryImage(info, { source, base, architecture }) {
    if (!/^sha256:[a-f0-9]{64}$/.test(info?.Id || '') || info.Os !== 'linux' || info.Architecture !== architecture
        || info.Config?.Labels?.['duskcue.qualification.kind'] !== 'local-playback-recovery-v1'
        || info.Config.Labels['duskcue.qualification.source-sha256'] !== source
        || info.Config.Labels['duskcue.qualification.runtime-base-id'] !== base
        || info.Config.User !== '65532:65532' || JSON.stringify(info.Config.Entrypoint) !== JSON.stringify(['/qualification/bin/playback-stop-contract'])) throw new Error('Recovery image source, base, platform or direct test entrypoint changed.');
    return { id: info.Id, architecture: info.Architecture, sourceSha256: source, runtimeBaseId: base };
}
