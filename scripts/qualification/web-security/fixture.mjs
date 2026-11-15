// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, lstat, realpath, readdir, readFile, writeFile, rm } from 'node:fs/promises';
import { dirname, join, relative, isAbsolute, resolve } from 'node:path';
import { readJson } from './prerequisite.mjs';

export const CACHE_VALUE = 'max-age=60"><script>globalThis.__duskcueCacheExecuted=1</script><meta data-x="&';

function samePath(a, b) { return process.platform === 'win32' ? resolve(a).toLowerCase() === resolve(b).toLowerCase() : resolve(a) === resolve(b); }

export function within(root, candidate) {
    const part = relative(root, candidate);
    return part !== '' && !isAbsolute(part) && part !== '..' && !part.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`);
}

export class OwnedFixture {
    #web;
    #nonce;
    #parent;
    path;
    constructor(web, nonce) { this.#web = resolve(web); this.#nonce = nonce; }
    async prepare(inputs) {
        assert.match(this.#nonce, /^[a-f0-9-]{36}$/);
        assert.ok(samePath(await realpath(this.#web), this.#web));
        let parent = this.#web;
        for (const name of ['.cache', 'security-boundaries']) {
            parent = join(parent, name);
            await mkdir(parent).catch((error) => { if (error.code !== 'EEXIST') throw error; });
            const info = await lstat(parent);
            assert.ok(info.isDirectory() && !info.isSymbolicLink() && samePath(await realpath(parent), parent), 'Fixture parent must not be a reparse path.');
        }
        this.#parent = parent;
        this.path = await mkdtemp(join(parent, 'run-'));
        await writeFile(join(this.path, '.owner.json'), JSON.stringify({ nonce: this.#nonce, pid: process.pid }), { flag: 'wx' });
        const files = {
            'package.json': JSON.stringify({ name: 'duskcue-security-fixture', private: true, type: 'module', devDependencies: inputs.versions }),
            'vite.config.js': "import { defineConfig } from 'vite'; import { sveltekit } from '@sveltejs/kit/vite'; export default defineConfig({ cacheDir: '.vite', plugins: [sveltekit()] });\n",
            'svelte.config.js': "import adapter from '@sveltejs/adapter-node'; export default { kit: { adapter: adapter({ out: 'build' }), prerender: { entries: ['/cache'] } } };\n",
            'src/app.html': '<!doctype html><html lang="en"><head><meta charset="utf-8">%sveltekit.head%</head><body><div>%sveltekit.body%</div></body></html>',
            'src/hooks.js': "export function reroute({ url }) { if (url.pathname.startsWith('/bad/') || url.pathname === '/alias' || url.pathname === '/fr/healthy') return '/healthy'; }\n",
            'src/routes/+page.svelte': '<h1>Boundary fixture</h1><a href="/healthy">Healthy response</a>',
            'src/routes/healthy/+server.js': `export function GET() { return new Response(${JSON.stringify(`healthy-${this.#nonce}`)}, { headers: { 'content-type': 'text/plain' } }); }\n`,
            'src/routes/cache/+page.js': `export const prerender = true; export function load({ setHeaders }) { setHeaders({ 'cache-control': ${JSON.stringify(CACHE_VALUE)} }); return {}; }\n`,
            'src/routes/cache/+page.svelte': '<h1>Cache fixture</h1><p>Ordinary prerendered content remains available.</p>',
        };
        for (const [name, content] of Object.entries(files)) {
            const path = join(this.path, name);
            await mkdir(dirname(path), { recursive: true });
            await writeFile(path, content, { flag: 'wx' });
        }
    }

    async remove() {
        if (!this.path) return;
        const target = resolve(this.path);
        assert.ok(this.#parent && within(this.#parent, target) && dirname(target) === this.#parent, 'Cleanup target is outside its owned root.');
        assert.ok(samePath(await realpath(target), target), 'Fixture cleanup target changed.');
        const marker = await readJson(join(target, '.owner.json'), 1024);
        assert.deepEqual(marker, { nonce: this.#nonce, pid: process.pid });
        let count = 0;
        const inspect = async (path) => {
            assert.ok(++count <= 10000, 'Generated fixture entry budget exceeded.');
            const info = await lstat(path);
            assert.ok(!info.isSymbolicLink(), 'Refusing fixture cleanup through a reparse path.');
            if (info.isDirectory()) for (const name of await readdir(path)) await inspect(join(path, name));
        };
        await inspect(target);
        assert.ok(samePath(await realpath(this.#parent), this.#parent) && samePath(await realpath(target), target));
        assert.equal((await readFile(join(target, '.owner.json'), 'utf8')), JSON.stringify(marker));
        await rm(target, { recursive: true });
        this.path = undefined;
    }
}
