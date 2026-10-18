// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, mkdir, writeFile, realpath, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { ownedWorkflowEvidence } from './workflow-evidence.mjs';

test('workflow evidence refuses a foreign marker, workload, parent or child before creating owned output', async () => {
    const temporaryRoot = await realpath(tmpdir());
    const workspace = await mkdtemp(join(temporaryRoot, 'duskcue-workflow-owner-'));
    const root = join(workspace, '.cache/testing-memory');
    await mkdir(root, { recursive: true });
    const resource = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
    const environment = { DUSKCUE_TEST_RESOURCE_ID: resource, DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE: join(root, `${resource}.containers.jsonl`) };
    const active = { pid: process.ppid, childPid: process.pid, workload: 'compose-build' };
    const save = (value) => writeFile(join(root, 'active.json'), JSON.stringify(value));
    try {
        await save(active);
        await assert.rejects(ownedWorkflowEvidence(workspace, environment, 'unregistered'));
        await assert.rejects(ownedWorkflowEvidence(workspace, { ...environment, DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE: join(workspace, 'foreign.jsonl') }, 'compose-build'));
        for (const invalid of [{ ...active, pid: process.ppid + 1 }, { ...active, childPid: process.pid + 1 }, { ...active, workload: 'playback-live-qualify' }]) {
            await save(invalid);
            await assert.rejects(ownedWorkflowEvidence(workspace, environment, 'compose-build'));
        }
        await save(active);
        const evidence = await ownedWorkflowEvidence(workspace, environment, 'compose-build');
        assert.equal(evidence.directory, join(root, `compose-build-${resource}`));
        assert.equal(evidence.resourceId, resource);
        assert.equal(await realpath(dirname(evidence.directory)), await realpath(root));
    } finally {
        const target = await realpath(workspace);
        assert.equal(dirname(target), temporaryRoot);
        assert.ok(target.startsWith(join(temporaryRoot, 'duskcue-workflow-owner-')) && resolve(target) !== temporaryRoot);
        await rm(target, { recursive: true });
    }
});
