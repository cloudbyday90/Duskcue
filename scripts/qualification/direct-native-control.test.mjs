// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, readFile, realpath, rename, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { createNativeStallControl } from '../../docker/qualification/direct-native-control.mjs';

const resourceId = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const report = { resourceId, sessionId: 'bbbbbbbb-bbbb-4ccc-8ddd-eeeeeeeeeeee', transcodeId: 'cccccccc-bbbb-4ccc-8ddd-eeeeeeeeeeee', positionMs: 4000, paused: true };
const identity = { pid: 123, parent_pid: 12, started_ticks: 456 };

async function fixture(run) {
    const parent = await realpath(tmpdir());
    const directory = await mkdtemp(join(parent, 'duskcue-native-control-'));
    try { await run(directory); }
    finally {
        const target = await realpath(directory);
        assert.equal(dirname(target), parent);
        assert.ok(target.startsWith(join(parent, 'duskcue-native-control-')));
        await rm(target, { recursive: true });
    }
}

test('private native control refuses disabled mode, foreign scope, hints and replay before writing', async () => {
    await fixture(async (directory) => {
        await assert.rejects(createNativeStallControl(resourceId, { directory })(report));
        for (const invalid of [{ ...report, resourceId: report.sessionId }, { ...report, positionMs: 6000 }, { ...report, paused: false }, { ...report, recovery_requested: true }, { ...report, transcodeId: 'foreign' }]) {
            await assert.rejects(createNativeStallControl(resourceId, { directory, enabled: true })(invalid));
        }
        await assert.rejects(readFile(join(directory, 'direct-native-stall.json')), { code: 'ENOENT' });
        await writeFile(join(directory, 'direct-native-stall-armed.json'), '{}');
        await assert.rejects(createNativeStallControl(resourceId, { directory, enabled: true })(report), /stale evidence/);
    });
});

test('private native arm publishes exact owned request then accepts only a matching bounded worker acknowledgement', async () => {
    for (const invalid of [false, true]) await fixture(async (directory) => {
        const arm = createNativeStallControl(resourceId, { directory, enabled: true });
        const pending = arm(report);
        pending.catch(() => {});
        const request = join(directory, 'direct-native-stall.json');
        const deadline = Date.now() + 2000;
        let bytes;
        while (!bytes && Date.now() < deadline) {
            try { bytes = await readFile(request, 'utf8'); }
            catch (error) { if (error.code !== 'ENOENT') throw error; }
            if (!bytes) await new Promise((resolve) => setTimeout(resolve, 5));
        }
        assert.deepEqual(JSON.parse(bytes), report);
        const response = { ...report, armed: true, encoderIdentity: { ...identity, started_ticks: invalid ? -1 : identity.started_ticks } };
        await writeFile(join(directory, 'armed.pending'), JSON.stringify(response));
        await rename(join(directory, 'armed.pending'), join(directory, 'direct-native-stall-armed.json'));
        if (invalid) await assert.rejects(pending, /owned worker/);
        else assert.deepEqual(await pending, response);
        await assert.rejects(arm(report), /owned scope/);
    });
});
