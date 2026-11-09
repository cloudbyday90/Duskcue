// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAndroidOwner } from './android-owner.mjs';
import { runAndroidApplication } from './android-app.mjs';
import { androidToolInputs, androidFailure } from './android-evidence.mjs';
import { runAndroidScopeApplication } from './android-scope-app.mjs';
import { runAndroidRelayApplication } from './android-relay-app.mjs';
import { runAndroidAmbientApplication } from './android-ambient-app.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
async function main() {
    if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(process.env.DUSKCUE_TEST_RESOURCE_ID ?? '')) throw new Error('Use a managed native resource identity.');
    const fixtureBytes = await readFile(process.env.DUSKCUE_VOD_CLIENT_FIXTURE);
    if (fixtureBytes.length > 4096) throw new Error('Private Android fixture is oversized.');
    const fixture = JSON.parse(fixtureBytes);
    if (fixture.suite !== undefined && !['scope', 'relay', 'ambient'].includes(fixture.suite)) throw new Error('Unknown native suite.');
    const directory = join(workspace, '.cache/android-playback', `native-${process.env.DUSKCUE_TEST_RESOURCE_ID}${fixture.suite ? `-${fixture.suite}` : ''}`);
    const deadlinePath = process.env.DUSKCUE_ANDROID_DEADLINE_FILE;
    if (!deadlinePath) throw new Error('Use the shared managed native deadline.');
    try { await writeFile(deadlinePath, String(Date.now() + 1800000), { flag: 'wx' }); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    const deadline = Number(await readFile(deadlinePath, 'utf8'));
    if (!Number.isSafeInteger(deadline) || deadline <= Date.now() || deadline > Date.now() + 1800000) throw new Error('Shared native deadline expired or invalid.');
    fixture.native_deadline = deadline;
    const inputs = await androidToolInputs(workspace);
    const owner = await createAndroidOwner(workspace, directory);
    let failure;
    const result = { status: 'in_progress', cleanup: false, tools: inputs };
    try { Object.assign(result, await (fixture.suite === 'scope' ? runAndroidScopeApplication : fixture.suite === 'relay' ? runAndroidRelayApplication : fixture.suite === 'ambient' ? runAndroidAmbientApplication : runAndroidApplication)(workspace, owner, fixture)); result.status = 'passed'; }
    catch (error) { failure = error; result.status = 'failed'; result.error = androidFailure(error, fixture.token); }
    finally {
        try { await owner.close(); result.cleanup = true; }
        catch (error) { failure ||= error; result.status = 'failed'; result.cleanupError = androidFailure(error, fixture.token); }
        if ((await androidToolInputs(workspace)).sha256 !== inputs.sha256) { failure ||= new Error('Android tooling changed during runtime.'); result.status = 'failed'; }
        await writeFile(join(directory, 'result.json'), JSON.stringify(result, null, 2));
    }
    process.stdout.write(`ANDROID_NATIVE_RESULT ${JSON.stringify({ status: result.status, cleanup: result.cleanup })}\n`);
    if (failure) throw failure;
}
main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
