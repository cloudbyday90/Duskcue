// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { appendFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ownedWorkflowEvidence } from './workflow-evidence.mjs';
import { evaluateLinuxHeadroom, readLinuxSample, readLocalEngine } from './linux-engine-headroom.mjs';
import { runCompilerCommand } from './linux-compile-command.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
let owner;
const result = { version: 1, kind: 'local-compose-no-cache-build', status: 'in_progress', startedAt: new Date().toISOString() };
try {
    owner = await ownedWorkflowEvidence(workspace, process.env, 'compose-build');
    result.resourceId = owner.resourceId;
    const engine = await readLocalEngine();
    result.engine = engine;
    const initial = await readLinuxSample(engine);
    const decision = evaluateLinuxHeadroom(initial, initial);
    result.linuxPreflight = { sample: initial, decision };
    if (!decision.allowed) throw new Error(decision.reason);
    const record = (sample) => appendFile(join(owner.directory, 'linux-memory.jsonl'), `${JSON.stringify(sample)}\n`);
    await record(initial);
    result.compilation = await runCompilerCommand(workspace, ['--context', engine.context, 'compose', '-f', 'docker-compose.yml', '-f', 'docker/compose.local.yml', '--progress', 'plain', 'build', '--no-cache', ...process.argv.slice(2)], owner, engine, initial, record);
    result.status = 'passed';
} catch (error) {
    result.status = 'failed'; result.error = error.message; process.exitCode = 1;
} finally {
    result.finishedAt = new Date().toISOString();
    if (owner) await writeFile(join(owner.directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);
}
console.log(JSON.stringify({ status: result.status, result: owner ? join(owner.directory, 'result.json') : null, ...(result.error ? { error: result.error } : {}) }));
