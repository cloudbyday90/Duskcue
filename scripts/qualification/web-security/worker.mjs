// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { once } from 'node:events';
import { mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { PATHS } from './patterns.mjs';

const [kind, web, nonce] = process.argv.slice(2);
if (kind === 'static') {
    const { platforms } = await import(pathToFileURL(join(web, 'node_modules/@sveltejs/adapter-static/platforms.js')).href);
    await mkdir('.vercel/output', { recursive: true });
    platforms.find((item) => item.name === 'Vercel').done({
        prerendered: {
            redirects: new Map(PATHS.map((path) => [path, { location: '/target', status: 308 }])),
            pages: new Map(PATHS.map((path, index) => [`${path}/`, { file: `page-${index}.html` }])),
        },
        getAppPath: () => '_app',
    });
} else if (kind === 'server' || kind === 'preview') {
    let native;
    if (kind === 'server') {
        const { server } = await import(pathToFileURL(join(process.cwd(), 'build/index.js')).href);
        native = server.server;
    } else {
        const { preview } = await import(pathToFileURL(join(web, 'node_modules/vite/dist/node/index.js')).href);
        const selected = await preview({ root: process.cwd(), configFile: 'vite.config.js', configLoader: 'native', preview: { host: '127.0.0.1', port: 0, strictPort: true } });
        native = selected.httpServer;
    }
    if (!native.listening) await once(native, 'listening');
    const address = native.address();
    if (!address || typeof address === 'string' || address.address !== '127.0.0.1') throw new Error('Unexpected fixture listener.');
    process.send({ kind: 'ready', nonce, pid: process.pid, origin: `http://127.0.0.1:${address.port}` });
} else throw new Error('Unknown private fixture worker.');
