// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { boundedMedia, fileIdentity } from './format-process.mjs';

test('format process rejects excess streamed output before unbounded capture', async () => {
    await assert.rejects(boundedMedia(process.execPath, ['-e', 'process.stdout.write(Buffer.alloc(65536));setInterval(()=>{},1000)'], { outputLimit: 1024, timeoutMs: 2000 }), /output bound/);
});

test('format process deadline waits for child exit and does not leave its worker alive', async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-format-process-'));
    const file = join(root, 'pid');
    try {
        await assert.rejects(boundedMedia(process.execPath, ['--input-type=module', '-e', 'import{writeFileSync}from"node:fs";writeFileSync(process.argv[1],String(process.pid));setInterval(()=>{},1000)', file], { timeoutMs: 1000 }), /deadline/);
        const pid = Number(await readFile(file, 'utf8'));
        assert.ok(Number.isInteger(pid) && pid > 0);
        assert.throws(() => process.kill(pid, 0), (error) => error.code === 'ESRCH');
    } finally { await rm(root, { recursive: true, force: true }); }
});

test('format process rejects missing executable and invalid bounds', async () => {
    await assert.rejects(boundedMedia('duskcue-format-no-such-executable', []), /could not start/);
    await assert.rejects(boundedMedia(process.execPath, [], { timeoutMs: 120001 }), /bounds/);
});

test('format file identity checks the byte ceiling and exact content', async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-format-file-'));
    try {
        const file = join(root, 'owned');
        await writeFile(file, 'abc', { flag: 'wx' });
        assert.deepEqual(await fileIdentity(file, 3), { bytes: 3, sha256: 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad' });
        await assert.rejects(fileIdentity(file, 2), /bound/);
        await assert.rejects(fileIdentity(root, 1000), /regular-file/);
    } finally { await rm(root, { recursive: true, force: true }); }
});

test('Linux format file identity refuses a symlink to an otherwise valid owned file', { skip: process.platform !== 'linux' }, async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-format-link-'));
    try {
        const file = join(root, 'owned');
        const link = join(root, 'link');
        await writeFile(file, 'abc', { flag: 'wx' });
        await symlink(file, link);
        await assert.rejects(fileIdentity(link, 3), /regular-file/);
    } finally { await rm(root, { recursive: true, force: true }); }
});
