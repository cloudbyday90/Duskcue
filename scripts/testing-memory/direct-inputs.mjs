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
        let info;
        try { info = await lstat(join(workspace, path)); }
        catch (error) { if (error.code === 'ENOENT' && path === 'clients/web/static') return; throw error; }
        if (info.isSymbolicLink()) throw new Error('Direct client sources must be regular.');
        if (info.isDirectory()) for (const entry of await readdir(join(workspace, path))) await visit(`${path}/${entry}`);
        else if (info.isFile() && info.size <= 1048576) files.push({ path, sha256: await hashFile(join(workspace, path)) });
        else throw new Error(`Direct client source exceeded its bound: ${path}. Regenerate production localization with the guarded desktop-build before qualification.`);
    }
    for (const path of ['clients/web/src', 'clients/web/static', 'clients/web/messages', 'clients/web/project.inlang/settings.json',
        'clients/web/package.json', 'clients/web/package-lock.json', 'clients/web/svelte.config.js', 'clients/web/vite.config.js', 'clients/web/jsconfig.json',
        'clients/web/tests/fixtures/direct-live-entry.mjs', 'clients/web/tests/fixtures/direct-live-observer.mjs', 'clients/web/tests/native',
        'clients/desktop/src', 'clients/desktop/scripts', 'clients/desktop/package.json', 'clients/desktop/package-lock.json', 'clients/desktop/vite.config.js',
        'clients/desktop/src-tauri/tauri.conf.json', 'clients/desktop/src-tauri/capabilities', 'clients/desktop/src-tauri/icons']) await visit(path);
    for (const entry of await readdir(join(workspace, 'clients/web/tests/fixtures'))) {
        if (/^format-[a-z0-9-]+\.mjs$/.test(entry)) await visit(`clients/web/tests/fixtures/${entry}`);
    }
    files.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
