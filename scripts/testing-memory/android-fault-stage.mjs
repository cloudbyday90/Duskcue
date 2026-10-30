// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createFaultGateway } from './android-fault-gateway.mjs';
import { runAndroidStage } from './android-stage.mjs';
import { androidTcpMapping } from './android-evidence.mjs';

export async function runAndroidFaultStage(workspace, owner, options) {
    const gateway = await createFaultGateway(options.fixture);
    const port = Number(new URL(gateway.origin).port);
    let reversed = false, failure;
    const directory = join(owner.directory, 'lost-outcomes');
    try {
        await mkdir(directory);
        gateway.arm({ deferRenew: true });
        await owner.command(['reverse', '--no-rebind', `tcp:${port}`, `tcp:${port}`]);
        reversed = true;
        const result = await runAndroidStage(workspace, owner, { ...options, directory,
            onPhase(line) { if (line.endsWith('ANDROID_PHASE renew-loss')) gateway.armRenew(); },
            fixture: { ...options.fixture, origin: gateway.origin, item: options.fixture.fault_item, scenario: 'lost-outcomes' } });
        gateway.verify();
        return result;
    } catch (error) { failure = error; throw error; }
    finally {
        const errors = [];
        try { await gateway.close(); } catch (error) { errors.push(error); }
        try { await writeFile(join(directory, 'http-witness.json'), JSON.stringify(gateway.trace)); } catch (error) { errors.push(error); }
        if (reversed) try {
            if (!androidTcpMapping(await owner.command(['reverse', '--list']), port)) throw new Error('Fault reverse lost its exact identity.');
            await owner.command(['reverse', '--remove', `tcp:${port}`]);
            if (androidTcpMapping(await owner.command(['reverse', '--list']), port)) throw new Error('Fault reverse remains.');
        } catch (error) { errors.push(error); }
        if (errors.length) throw new AggregateError([...(failure ? [failure] : []), ...errors], 'Fault stage cleanup is unconfirmed.');
    }
}
