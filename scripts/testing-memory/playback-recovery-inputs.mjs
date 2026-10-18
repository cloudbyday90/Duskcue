// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { copyFile, lstat, mkdir, readdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { hashFile, qualificationSources } from '../qualification/source.mjs';
import { ownedWorkflowEvidence } from './workflow-evidence.mjs';

export function parseRecoveryArguments(args) {
    if (args.length !== 2 || args[0] !== '--image' || !/^sha256:[a-f0-9]{64}$/.test(args[1])) throw new Error('Provide --image <immutable final local Duskcue image SHA256>.');
    return { image: args[1] };
}

export function parseRecoveryInvocation(args) {
    const nativeRecovery = args[0] === '--native-recovery';
    const liveClients = nativeRecovery || args[0] === '--live-clients';
    const directBrowser = liveClients || args[0] === '--direct-browser';
    if (liveClients && (args[1] !== '--native-manifest' || !args[2])) throw new Error('Live clients require --native-manifest <freshly built qualification manifest>.');
    return { ...parseRecoveryArguments(args.slice(liveClients ? 3 : directBrowser ? 1 : 0)), nativeRecovery, liveClients, directBrowser,
        nativeManifestPath: liveClients ? args[2] : null,
        workload: nativeRecovery ? 'playback-native-recovery-qualify' : liveClients ? 'playback-live-qualify' : directBrowser ? 'playback-direct-qualify' : 'playback-recovery-qualify' };
}

export async function owningRecoveryDirectory(workspace, environment, workload = 'playback-recovery-qualify') {
    if (!['playback-recovery-qualify', 'playback-direct-qualify', 'playback-live-qualify', 'playback-native-recovery-qualify'].includes(workload)) throw new Error('An owning recovery workflow is required.');
    return ownedWorkflowEvidence(workspace, environment, workload);
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

export async function copyRecoverySnapshot(workspace, directory, expected, sources = recoverySources) {
    await mkdir(directory);
    for (const file of expected.files) {
        const destination = join(directory, file.path);
        await mkdir(dirname(destination), { recursive: true });
        await copyFile(join(workspace, file.path), destination);
    }
    await writeFile(join(directory, '.dockerignore'), '.git\nnode_modules\n**/node_modules\ntarget\n**/target\n.env\n.env.*\n');
    if ((await sources(directory)).sha256 !== expected.sha256) throw new Error('Recovery sources changed while copying the current working tree.');
}

export function validateRecoveryImage(info, { source, base, architecture, nativeRecovery = false }) {
    if (!/^sha256:[a-f0-9]{64}$/.test(info?.Id || '') || info.Os !== 'linux' || info.Architecture !== architecture
        || info.Config?.Labels?.['duskcue.qualification.kind'] !== 'local-playback-recovery-v1'
        || info.Config.Labels['duskcue.qualification.source-sha256'] !== source
        || info.Config.Labels['duskcue.qualification.runtime-base-id'] !== base
        || info.Config.Labels['duskcue.qualification.native-recovery'] !== (nativeRecovery ? 'paced' : 'none')
        || info.Config.User !== '65532:65532' || JSON.stringify(info.Config.Entrypoint) !== JSON.stringify(['/qualification/bin/playback-stop-contract'])) throw new Error('Recovery image source, base, platform or direct test entrypoint changed.');
    return { id: info.Id, architecture: info.Architecture, sourceSha256: source, runtimeBaseId: base, nativeRecoveryMode: nativeRecovery ? 'paced' : 'none' };
}
