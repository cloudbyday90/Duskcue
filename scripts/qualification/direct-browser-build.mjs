// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { access, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function prepareDirectPlayerBrowser(workspace, directory) {
    const web = join(workspace, 'clients/web');
    await access(join(web, 'src/lib/paraglide/runtime.js'));
    await access(join(web, 'src/lib/paraglide/messages/_index.js'));
    const { build } = await import(pathToFileURL(join(web, 'node_modules/vite/dist/node/index.js')));
    const { svelte } = await import(pathToFileURL(join(web, 'node_modules/@sveltejs/vite-plugin-svelte/src/index.js')));
    const output = join(directory, 'browser');
    await mkdir(output);
    await build({ configFile: false, root: web, logLevel: 'warn',
        plugins: [svelte({ configFile: false })], resolve: { alias: { $lib: join(web, 'src/lib') } },
        build: { outDir: output, emptyOutDir: false, minify: false, sourcemap: false, reportCompressedSize: false,
            target: 'es2022', assetsDir: '.', cssCodeSplit: false,
            rollupOptions: { input: join(web, 'tests/fixtures/direct-live-entry.mjs'),
                output: { entryFileNames: 'direct.js', chunkFileNames: '[name]-[hash].js', assetFileNames: 'direct[extname]' } } } });
    return output;
}
