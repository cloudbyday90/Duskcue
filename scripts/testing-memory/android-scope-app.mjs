// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createFaultGateway } from './android-fault-gateway.mjs';
import { runAndroidApplication } from './android-app.mjs';

export async function runAndroidScopeApplication(workspace, owner, fixture) {
    const tokens = [fixture.token, fixture.peer_token, fixture.other_token];
    const primary = await createFaultGateway(fixture, { scopeTokens: tokens });
    let alternate, failure;
    try {
        alternate = await createFaultGateway(fixture, { scopeTokens: tokens });
        const result = await runAndroidApplication(workspace, owner, { ...fixture, origin: primary.origin, alternate_origin: alternate.origin });
        primary.verifyScope(3, true);
        alternate.verifyScope(1, false);
        await writeFile(join(owner.directory, 'scope-http.json'), JSON.stringify({ primary: primary.trace, alternate: alternate.trace }, null, 2));
        return result;
    } catch (error) { failure = error; throw error; }
    finally {
        const settled = await Promise.allSettled([primary.close(), alternate?.close()]);
        const errors = settled.filter((result) => result.status === 'rejected').map((result) => result.reason);
        if (errors.length) throw new AggregateError([...(failure ? [failure] : []), ...errors], 'Scope HTTP cleanup is unconfirmed.');
    }
}
