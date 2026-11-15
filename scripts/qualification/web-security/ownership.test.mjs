// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter, once } from 'node:events';
import { PassThrough } from 'node:stream';
import { mkdir, mkdtemp, readFile, writeFile, rmdir, symlink, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { qualifyBoundaries, observe } from './lifecycle.mjs';
import { EXPECTED, SUPPORT, validateInstallation } from './prerequisite.mjs';
import { ProcessOwners } from './process.mjs';
import { OwnedFixture, CACHE_VALUE } from './fixture.mjs';
import { routeMatches, assertLiteralPatterns } from './patterns.mjs';
import { assertCacheAttribute } from './cases.mjs';

const ok = () => ({ reroute: true, staticPatterns: true, cacheAttribute: true, completeSecurityMatrix: true });
const signal = () => new AbortController().signal;
const deferred = () => { let resolve; const promise = new Promise((done) => { resolve = done; }); return { promise, resolve }; };
const tick = () => new Promise((resolve) => setImmediate(resolve));

function operations(events, changes = {}) {
    return {
        prerequisite: async () => { events.push('prerequisite'); return {}; },
        prepare: async () => { events.push('prepare'); },
        build: async () => { events.push('build'); },
        run: async () => { events.push('run'); return ok(); },
        dispose: async () => { events.push('dispose'); },
        ...changes,
    };
}

test('failed installation prerequisite cannot create fixtures, builds or listeners', async () => {
    const events = [];
    await assert.rejects(qualifyBoundaries(operations(events, { prerequisite: async () => { throw new Error('old installed graph'); } }), signal()), /old installed graph/);
    assert.deepEqual(events, []);
});

test('partial fixture evidence and cleanup cannot become complete security qualification', async () => {
    const events = [];
    const result = await qualifyBoundaries(operations(events), signal());
    assert.deepEqual(events, ['prerequisite', 'prepare', 'build', 'run', 'dispose']);
    assert.equal(result.cleanupConfirmed, true);
    assert.equal(result.completeSecurityMatrix, false);
});

for (const stage of ['prepare', 'build', 'run']) test(`${stage} failure still joins cleanup and preserves the original error`, async () => {
    const events = [];
    const failure = new Error(stage);
    await assert.rejects(qualifyBoundaries(operations(events, { [stage]: async () => { events.push(stage); throw failure; } }), signal()), (error) => error === failure);
    assert.equal(events.at(-1), 'dispose');
});

test('cancellation while preparation completes late still observes and cleans its resource', async () => {
    const controller = new AbortController();
    const entered = deferred();
    const held = deferred();
    const events = [];
    const work = qualifyBoundaries(operations(events, { prepare: async () => { entered.resolve(); await held.promise; events.push('late-created'); } }), controller.signal);
    const rejected = assert.rejects(work, /cancelled/);
    await entered.promise;
    controller.abort(new Error('cancelled'));
    await tick();
    assert.deepEqual(events, ['prerequisite']);
    held.resolve();
    await rejected;
    assert.deepEqual(events, ['prerequisite', 'late-created', 'dispose']);
});

test('execution and cleanup failures are retained together rather than reporting a pass', async () => {
    const first = new Error('execution');
    const second = new Error('cleanup');
    await assert.rejects(qualifyBoundaries(operations([], { run: async () => { throw first; }, dispose: async () => { throw second; } }), signal()),
        (error) => error instanceof AggregateError && error.errors[0] === first && error.errors[1] === second);
});

test('cancellation during late cleanup cannot become a successful partial marker', async () => {
    const controller = new AbortController();
    const entered = deferred();
    const held = deferred();
    const work = qualifyBoundaries(operations([], { dispose: async () => { entered.resolve(); await held.promise; } }), controller.signal);
    const rejected = assert.rejects(work, /cancelled during cleanup/);
    await entered.promise;
    controller.abort(new Error('cancelled during cleanup'));
    held.resolve();
    await rejected;
});

test('incomplete executed case markers fail even after successful cleanup', async () => {
    const events = [];
    await assert.rejects(qualifyBoundaries(operations(events, { run: async () => ({ ...ok(), reroute: false }) }), signal()), /incomplete/);
    assert.equal(events.at(-1), 'dispose');
});

test('an observer timeout retains the underlying operation until actual completion', async () => {
    const held = deferred();
    let finished = false;
    held.promise.then(() => { finished = true; });
    await assert.rejects(observe(held.promise, 10, 'unconfirmed'), /unconfirmed/);
    assert.equal(finished, false);
    held.resolve();
    await held.promise;
    assert.equal(finished, true);
});

test('installed identities and every supporting version must match the locked candidate', () => {
    const versions = { ...EXPECTED, ...Object.fromEntries(SUPPORT.map((name) => [name, '1.2.3'])) };
    const manifest = { dependencies: { '@sveltejs/adapter-static': '^3.0.11' }, devDependencies: { '@sveltejs/kit': '^2.70.4' } };
    const lock = { lockfileVersion: 3, packages: { '': structuredClone(manifest), ...Object.fromEntries(Object.entries(versions).map(([name, version]) => [`node_modules/${name}`, { version }])) } };
    const installed = Object.fromEntries(Object.entries(versions).map(([name, version]) => [name, { name, version }]));
    assert.deepEqual(validateInstallation(manifest, lock, installed), versions);
    for (const name of Object.keys(installed)) {
        const changed = structuredClone(installed);
        changed[name].version = '0.0.0';
        assert.throws(() => validateInstallation(manifest, lock, changed), /Install the locked graph/);
    }
    assert.throws(() => validateInstallation(manifest, lock, {}));
    const changed = structuredClone(lock);
    changed.packages['node_modules/@sveltejs/kit'].version = '3.1.0';
    assert.throws(() => validateInstallation(manifest, changed, installed), /Unexpected locked/);
});

class FakeChild extends EventEmitter {
    pid = 12345;
    stdout = new PassThrough();
    stderr = new PassThrough();
    exitCode = null;
    signalCode = null;
    close(code = 0) { this.exitCode = code; this.emit('close', code, null); }
}

test('process ownership survives exit and a cleanup observer timeout until stdio close', async () => {
    const child = new FakeChild();
    let stops = 0;
    const owners = new ProcessOwners({ start: () => child, stop: async () => { stops++; }, stopTimeoutMs: 15 });
    const entry = owners.start([]);
    child.exitCode = 0;
    child.emit('exit', 0, null);
    assert.equal(owners.pending, 1);
    assert.throws(() => owners.start([]), /still pending/);
    await assert.rejects(entry.stop(), /unconfirmed/);
    assert.equal(owners.pending, 1);
    child.close();
    await entry.closed;
    await entry.stop();
    await owners.close();
    assert.equal(stops, 1);
    assert.equal(owners.pending, 0);
});

test('spawn errors retain their registration until close and fail command execution', async () => {
    const child = new FakeChild();
    const owners = new ProcessOwners({ start: () => child, stop: async () => child.close(1) });
    const entry = owners.start([]);
    child.emit('error', new Error('spawn failed'));
    assert.equal(owners.pending, 1);
    child.close(1);
    await assert.rejects(entry.wait(), /spawn failed/);
    await owners.close();
});

test('readiness is bound to nonce, exact child and literal ephemeral origin', async () => {
    for (const changed of [{ nonce: 'stale' }, { pid: 99999 }, { origin: 'http://localhost:1234' }, { origin: 'http://127.0.0.1:1234/path' }]) {
        const child = new FakeChild();
        const owners = new ProcessOwners({ start: () => child, stop: async () => child.close() });
        const entry = owners.start([], { nonce: 'current' });
        const pending = assert.rejects(entry.ready());
        child.emit('message', { kind: 'ready', nonce: 'current', pid: child.pid, origin: 'http://127.0.0.1:1234', ...changed });
        await pending;
        await owners.close();
        assert.throws(() => owners.assertHealthy());
    }
});

test('early exit, readiness timeout and duplicate readiness cannot be adopted', async () => {
    for (const mode of ['early', 'timeout', 'duplicate']) {
        const child = new FakeChild();
        const owners = new ProcessOwners({ start: () => child, stop: async () => child.close() });
        const entry = owners.start([], { nonce: 'current' });
        if (mode === 'early') { child.close(1); await assert.rejects(entry.ready()); }
        if (mode === 'timeout') await assert.rejects(entry.ready(10), /readiness deadline/);
        if (mode === 'duplicate') {
            const message = { kind: 'ready', nonce: 'current', pid: child.pid, origin: 'http://127.0.0.1:1234' };
            child.emit('message', message);
            assert.equal(await entry.ready(), message.origin);
            child.emit('message', message);
            assert.throws(() => owners.assertHealthy(), /duplicate/);
        }
        await owners.close();
    }
});

test('combined output overflow and cancellation request stop while retaining failure', async () => {
    for (const mode of ['output', 'abort']) {
        const controller = new AbortController();
        const child = new FakeChild();
        let stops = 0;
        const owners = new ProcessOwners({ signal: controller.signal, start: () => child, stop: async () => { stops++; child.close(); }, outputBytes: 8 });
        const entry = owners.start([]);
        if (mode === 'output') { child.stdout.write('12345'); child.stderr.write('6789'); }
        else controller.abort(new Error('cancelled'));
        await assert.rejects(entry.wait());
        await owners.close();
        assert.equal(stops, 1);
        assert.equal(owners.pending, 0);
    }
});

test('an overflowing producer cannot allocate a cleanup observer for every further chunk', async (context) => {
    const child = new FakeChild();
    const owners = new ProcessOwners({ start: () => child, stop: async () => child.close(), outputBytes: 8 });
    const nativeTimeout = globalThis.setTimeout;
    let scheduled = 0;
    context.mock.method(globalThis, 'setTimeout', (...args) => { scheduled++; return nativeTimeout(...args); });
    const entry = owners.start([]);
    try {
        for (let index = 0; index < 256; index++) child.stdout.write('123456789');
        assert.equal(scheduled, 1);
        await assert.rejects(entry.wait(), /output budget/);
    } finally { await owners.close(); }
});

test('joins a real direct Node child and stdio after owned tree termination', async () => {
    const owners = new ProcessOwners();
    const entry = owners.start(['-e', "process.stdout.write('ready'); setInterval(() => {}, 1000)"]);
    try {
        await observe(once(entry.child.stdout, 'data'), 5000, 'Child did not start.');
        assert.equal(owners.pending, 1);
        await entry.stop();
        assert.equal(owners.pending, 0);
    } finally { await owners.close(); }
});

test('literal-path oracle catches raw terminal dollar and metacharacter overmatching', () => {
    assert.equal(routeMatches('/terminal$', '/terminal$'), false);
    assert.equal(routeMatches('/terminal$', '/terminal'), true);
    assert.equal(routeMatches('/terminal\\x24', '/terminal$'), true);
    assert.equal(routeMatches('/dot.name', '/dotXname'), true);
    assert.throws(() => assertLiteralPatterns({ version: 3, routes: [] }));
});

test('cache evidence requires one intact escaped attribute and ordinary content', () => {
    const escaped = CACHE_VALUE.replaceAll('&', '&amp;').replaceAll('"', '&quot;');
    assert.doesNotThrow(() => assertCacheAttribute(`<h1>Cache fixture</h1><meta http-equiv="cache-control" content="${escaped}">`));
    assert.throws(() => assertCacheAttribute(`<h1>Cache fixture</h1><meta http-equiv="cache-control" content="${CACHE_VALUE}">`));
    assert.throws(() => assertCacheAttribute('unrelated 404'));
});

test('fixture cleanup refuses a changed owner and a reparse path without touching its target', async () => {
    const cache = fileURLToPath(new URL('../../../.cache/testing-memory/', import.meta.url));
    await mkdir(cache, { recursive: true });
    const web = await mkdtemp(join(cache, 'web-security-unit-'));
    const nonce = '00000000-0000-4000-8000-000000000000';
    const fixture = new OwnedFixture(web, nonce);
    let link;
    const outside = join(web, 'outside');
    const original = JSON.stringify({ nonce, pid: process.pid });
    try {
        await fixture.prepare({ web, versions: {} });
        await writeFile(join(fixture.path, '.owner.json'), '{}');
        await assert.rejects(fixture.remove());
        await writeFile(join(fixture.path, '.owner.json'), original);
        await mkdir(outside);
        await writeFile(join(outside, 'sentinel'), 'keep');
        link = join(fixture.path, 'linked');
        await symlink(outside, link, process.platform === 'win32' ? 'junction' : 'dir');
        await assert.rejects(fixture.remove(), /reparse path/);
        assert.equal(await readFile(join(outside, 'sentinel'), 'utf8'), 'keep');
    } finally {
        if (link) await unlink(link);
        if (fixture.path) { await writeFile(join(fixture.path, '.owner.json'), original); await fixture.remove(); }
        await unlink(join(outside, 'sentinel')).catch((error) => { if (error.code !== 'ENOENT') throw error; });
        await rmdir(outside).catch((error) => { if (error.code !== 'ENOENT') throw error; });
        await rmdir(join(web, '.cache/security-boundaries'));
        await rmdir(join(web, '.cache'));
        await rmdir(web);
    }
});
