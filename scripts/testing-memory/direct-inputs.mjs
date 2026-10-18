// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { lstat, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { hashFile } from '../qualification/source.mjs';
import { recoverySources } from './playback-recovery-inputs.mjs';

export async function directSources(workspace) {
    const { files } = await recoverySources(workspace);
    async function visit(path) {
        const info = await lstat(join(workspace, path));
        if (info.isSymbolicLink()) throw new Error('Direct client sources must be regular.');
        if (info.isDirectory()) for (const entry of await readdir(join(workspace, path))) await visit(`${path}/${entry}`);
        else if (info.isFile() && info.size <= 1048576) files.push({ path, sha256: await hashFile(join(workspace, path)) });
        else throw new Error('Direct client source exceeded its bound.');
    }
    for (const path of ['clients/web/src/lib/playback', 'clients/web/src/lib/stores', 'clients/web/src/lib/api', 'clients/web/src/lib/preferences', 'clients/web/package.json', 'clients/web/package-lock.json', 'clients/web/tests/fixtures/direct-live-entry.mjs']) await visit(path);
    files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
