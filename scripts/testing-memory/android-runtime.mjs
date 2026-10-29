// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createAndroidOwner } from './android-owner.mjs';
import { runAndroidApplication } from './android-app.mjs';
import { androidToolInputs } from './android-evidence.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
async function main() {
    const directory = join(workspace, '.cache/android-playback', `native-${process.env.DUSKCUE_TEST_RESOURCE_ID}`);
    const fixtureBytes = await readFile(process.env.DUSKCUE_VOD_CLIENT_FIXTURE);
    if (fixtureBytes.length > 4096) throw new Error('Private Android fixture is oversized.');
    const fixture = JSON.parse(fixtureBytes);
    const inputs = await androidToolInputs(workspace);
    const owner = await createAndroidOwner(workspace, directory);
    let failure;
    const result = { status: 'in_progress', cleanup: false, tools: inputs };
    try { Object.assign(result, await runAndroidApplication(workspace, owner, fixture)); result.status = 'passed'; }
    catch (error) { failure = error; result.status = 'failed'; result.error = error.message; }
    finally {
        try { await owner.close(); result.cleanup = true; }
        catch (error) { failure ||= error; result.status = 'failed'; result.cleanupError = error.message; }
        if ((await androidToolInputs(workspace)).sha256 !== inputs.sha256) { failure ||= new Error('Android tooling changed during runtime.'); result.status = 'failed'; }
        await writeFile(join(directory, 'result.json'), JSON.stringify(result, null, 2));
    }
    process.stdout.write(`ANDROID_NATIVE_RESULT ${JSON.stringify({ status: result.status, cleanup: result.cleanup })}\n`);
    if (failure) throw failure;
}
main().catch((error) => { process.stderr.write(`${error.message}\n`); process.exitCode = 1; });
