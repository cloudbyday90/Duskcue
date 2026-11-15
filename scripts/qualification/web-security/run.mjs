// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomUUID } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { qualifyBoundaries, observe } from './lifecycle.mjs';
import { installedInputs } from './prerequisite.mjs';
import { OwnedFixture } from './fixture.mjs';
import { ProcessOwners, fixtureEnvironment } from './process.mjs';
import { LoopbackRequests } from './http.mjs';
import { runCases } from './cases.mjs';

const web = fileURLToPath(new URL('../../../clients/web/', import.meta.url));
if (process.argv.length !== 2) throw new Error('The boundary runner accepts no overrides.');
const nonce = randomUUID();
const controller = new AbortController();
const timer = setTimeout(() => controller.abort(new Error('Security boundary qualification deadline exceeded.')), 600000);
const interrupt = () => controller.abort(new Error('Security boundary qualification interrupted.'));
process.on('SIGINT', interrupt);
process.on('SIGTERM', interrupt);
const owners = new ProcessOwners({ signal: controller.signal });
const requests = new LoopbackRequests();
const fixture = new OwnedFixture(web, nonce);
let inputs;
try {
    const result = await qualifyBoundaries({
        prerequisite: async (signal) => (inputs = await installedInputs(web, signal)),
        prepare: (selected) => fixture.prepare(selected),
        build: async (selected) => {
            const build = owners.start([join(selected.web, 'node_modules/vite/bin/vite.js'), 'build', '--configLoader', 'native'], { cwd: fixture.path, env: fixtureEnvironment({ NODE_ENV: 'production' }) });
            await build.wait();
        },
        run: async (selected, signal) => {
            const result = await runCases(selected, fixture, owners, requests, nonce, signal);
            assert.deepEqual(await installedInputs(web, signal), selected, 'Qualification input identity changed.');
            return result;
        },
        dispose: async () => {
            const settled = await Promise.allSettled([observe(requests.close(), 10000, 'HTTP cleanup is unconfirmed; fixture retained.'), owners.close()]);
            const failures = settled.filter((item) => item.status === 'rejected').map((item) => item.reason);
            if (failures.length) throw new AggregateError(failures, 'Owned cleanup is unconfirmed; fixture retained.');
            await fixture.remove();
            owners.assertHealthy();
        },
    }, controller.signal);
    console.log(`DUSKCUE_WEB_SECURITY_BOUNDARIES=${JSON.stringify({ version: 1, nonce, ...result, versions: inputs.versions, lockSha256: inputs.lockSha256 })}`);
} catch (error) {
    console.error(error.message);
    if (fixture.path) console.error(`Retained owned fixture: ${fixture.path}`);
    if (owners.tail) console.error(owners.tail);
    process.exitCode = 1;
} finally {
    clearTimeout(timer);
    process.off('SIGINT', interrupt);
    process.off('SIGTERM', interrupt);
}
