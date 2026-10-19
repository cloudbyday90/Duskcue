// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { lstat, mkdir, readFile, realpath } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';

const directories = Object.freeze({ 'compose-build': 'compose-build', 'playback-recovery-qualify': 'playback-recovery', 'playback-direct-qualify': 'playback-recovery', 'playback-live-qualify': 'playback-recovery', 'playback-native-recovery-qualify': 'playback-recovery', 'playback-format-qualify': 'playback-recovery', 'playback-source-qualify': 'playback-recovery' });

export async function ownedWorkflowEvidence(workspace, environment, workload) {
    if (!Object.hasOwn(directories, workload)) throw new Error('An admitted owning workflow is required.');
    const root = await realpath(join(workspace, '.cache/testing-memory'));
    const resourceId = environment.DUSKCUE_TEST_RESOURCE_ID;
    const marker = environment.DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE;
    if (!/^[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/.test(resourceId || '') || !marker || resolve(marker) !== join(root, `${resourceId}.containers.jsonl`)) throw new Error('The workflow requires its owning resource UUID and container marker.');
    const deadline = Date.now() + 1000;
    let active;
    do {
        active = JSON.parse(await readFile(join(root, 'active.json'), 'utf8'));
        if (active.pid !== process.ppid || active.workload !== workload) throw new Error('The workflow must be the child of its registered owner.');
        if (active.childPid === process.pid) break;
        if (active.childPid !== null) throw new Error('The workflow owns a different child.');
        await new Promise((resume) => setTimeout(resume, 20));
    } while (Date.now() < deadline);
    if (active.childPid !== process.pid) throw new Error('The workflow child was not registered.');
    const directory = join(root, `${directories[workload]}-${resourceId}`);
    await mkdir(directory);
    if (dirname(await realpath(directory)) !== root || (await lstat(directory)).isSymbolicLink()) throw new Error('Workflow evidence escaped its owning workspace.');
    return { resourceId, marker, directory };
}
