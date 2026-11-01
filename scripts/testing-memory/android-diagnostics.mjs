// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { redactAndroidOutput } from './android-evidence.mjs';

export async function captureAndroidFailure(owner, directory, fixture) {
    const secrets = [fixture.token, fixture.peer_token, fixture.other_token].filter(Boolean);
    const clean = (value) => redactAndroidOutput(secrets.reduce((text, secret) => text.replaceAll(secret, '[redacted]'), value));
    const result = {};
    const deadline = Math.min(fixture.native_deadline ?? Infinity, Date.now() + 10000);
    for (const [name, args] of [
        ['logcat', ['logcat', '-d', '-v', 'threadtime', '-t', '200']],
        ['appExit', ['shell', 'dumpsys', 'activity', 'exit-info', 'com.duskcue.mobile.qualification']],
    ]) {
        try {
            const remaining = deadline - Date.now();
            if (remaining <= 0) throw new Error('Diagnostic deadline expired.');
            result[name] = clean(await owner.command(args, { timeoutMs: Math.min(5000, remaining), bytes: 65536 }));
        }
        catch { result[name] = 'Bounded diagnostic unavailable; no app-completion claim.'; }
    }
    await writeFile(join(directory, 'failure-diagnostics.json'), JSON.stringify(result, null, 2));
}
