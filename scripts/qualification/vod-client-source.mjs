// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { lstat } from 'node:fs/promises';
import { join } from 'node:path';
import { runBounded } from './commands.mjs';
import { hashFile } from './source.mjs';

export async function vodClientSources(workspace) {
    const inventory = await runBounded('git', ['ls-files', '-co', '--exclude-standard', '-z', '--',
        'clients/web', 'clients/desktop', 'scripts/testing-memory/qualify-vod-output.mjs', 'scripts/testing-memory/presets.mjs'],
    { cwd: workspace, timeoutMs: 10000, bytes: 2097152 });
    if (inventory.code !== 0 || inventory.signal) throw new Error('Client source inventory unavailable.');
    const paths = [...new Set(inventory.stdout.split('\0').filter(Boolean))].sort();
    if (!paths.length || paths.length > 10000) throw new Error('Client source inventory exceeds its bound.');
    const files = [];
    for (const path of paths) {
        if (path.length > 4096 || path.startsWith('/') || path.split('/').includes('..')) throw new Error('Invalid client source path.');
        const metadata = await lstat(join(workspace, path));
        if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 268435456) throw new Error('Client source is not a bounded regular file.');
        files.push({ path, sha256: await hashFile(join(workspace, path)) });
    }
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
