// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { lstat, readdir } from 'node:fs/promises';
import { join } from 'node:path';
import { hashFile } from '../qualification/source.mjs';

export async function flutterInputs(workspace) {
    const files = [];
    async function visit(path) {
        if (['clients/mobile/android/local.properties', 'clients/mobile/android/app/src/main/java/io/flutter/plugins/GeneratedPluginRegistrant.java'].includes(path)) return;
        const info = await lstat(join(workspace, path));
        if (info.isSymbolicLink()) throw new Error('Flutter inputs must be regular files.');
        if (info.isDirectory()) {
            for (const name of (await readdir(join(workspace, path))).sort()) {
                if (['.gradle', '.cxx', 'build', '.kotlin'].includes(name)) continue;
                await visit(`${path}/${name}`);
            }
        } else if (info.isFile() && info.size <= 16 * 1024 ** 2 && files.length < 4096) {
            files.push({ path, sha256: await hashFile(join(workspace, path)) });
        } else throw new Error('Flutter source inventory exceeded its finite limit.');
    }
    for (const path of ['clients/mobile/lib', 'clients/mobile/test', 'clients/mobile/integration_test', 'clients/mobile/android',
        'clients/mobile/pubspec.yaml', 'clients/mobile/pubspec.lock', 'clients/mobile/analysis_options.yaml', 'scripts/testing-memory/flutter-command.mjs',
        'scripts/testing-memory/flutter-inputs.mjs', 'scripts/testing-memory/flutter-toolchain.json']) await visit(path);
    return { files, sha256: createHash('sha256').update(JSON.stringify(files)).digest('hex') };
}
