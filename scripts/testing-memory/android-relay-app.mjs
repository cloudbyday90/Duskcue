// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createFaultGateway } from './android-fault-gateway.mjs';
import { createRelayWitness } from './android-relay-witness.mjs';
import { runAndroidApplication } from './android-app.mjs';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';

export async function runAndroidRelayApplication(workspace, owner, fixture) {
    const witness = createRelayWitness(fixture, owner.directory);
    const gateway = await createFaultGateway(fixture, { relayWitness: witness });
    try {
        const result = await runAndroidApplication(workspace, owner, { ...fixture, origin: gateway.origin });
        if (witness.probes.length !== 3 || new Set(witness.probes.map((p) => p.start)).size !== 1
            || new Set(witness.probes.map((p) => p.output)).size !== 3 || witness.stats.rootMedia < 1
            || witness.stats.master < 2 || witness.stats.variant < 2) throw new Error('Native format attribution differs.');
        await writeFile(join(owner.directory, 'relay-http.json'), JSON.stringify(witness.stats));
        return result;
    } finally { await gateway.close(); await witness.close(); }
}
