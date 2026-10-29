// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { writeFile } from 'node:fs/promises';
import { createAndroidOwner } from './android-owner.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
async function main() {
    const directory = join(workspace, '.cache/android-playback', `boot-${process.env.DUSKCUE_TEST_RESOURCE_ID}`);
    const owner = await createAndroidOwner(workspace, directory);
    try { process.stdout.write('Owned API 36 x86_64 emulator booted with WHPX and SwiftShader.\n'); }
    finally { await owner.close(); }
    await writeFile(join(directory, 'result.json'), JSON.stringify({ boot: true, cleanup: true, completedAt: new Date().toISOString() }));
    process.stdout.write('Owned emulator and isolated ADB shutdown confirmed.\n');
}
main().catch((error) => { process.stderr.write(`${error.message || error.errors?.map((item) => item.message).join('; ')}\n`); process.exitCode = 1; });
