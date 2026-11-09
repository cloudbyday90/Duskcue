// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createFaultGateway } from './android-fault-gateway.mjs';
import { runAndroidApplication } from './android-app.mjs';

export async function runAndroidAmbientApplication(workspace, owner, fixture) {
    const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
    const channels = [fixture.channel, fixture.next_channel];
    if (!channels.every((value) => uuid.test(value)) || channels[0] === channels[1]) throw new Error('Use two exact owned ambient channels.');
    const credentials = [fixture.token, fixture.peer_token];
    const options = { scopeTokens: credentials, ambientChannels: channels };
    const http = await createFaultGateway(fixture, options);
    let secure;
    try {
        secure = await createFaultGateway(fixture, { ...options, tls: {
            cert: await readFile(join(workspace, 'clients/mobile/test/fixtures/relay-localhost-cert.pem')),
            key: await readFile(join(workspace, 'clients/mobile/test/fixtures/relay-localhost-key.pem')),
            minVersion: 'TLSv1.2',
        } });
        const result = await runAndroidApplication(workspace, owner, { ...fixture, origin: secure.origin, http_origin: http.origin });
        const traces = [http.trace, secure.trace];
        const starts = traces.flat().filter((entry) => entry.method === 'POST' && entry.path === '/api/v1/playback-commands');
        if (!http.trace.some((entry) => entry.path === '/api/v1/playback-commands' && entry.status === 202)
            || !secure.trace.some((entry) => entry.path.endsWith('.m4s') && entry.complete)
            || starts.some((entry) => entry.status !== 202 || !entry.complete)) throw new Error('Ambient native HTTP/HTTPS attribution is incomplete.');
        await writeFile(join(owner.directory, 'ambient-http.json'), JSON.stringify({
            starts: starts.length, http: http.trace, https: secure.trace,
        }));
        return result;
    } finally { await Promise.allSettled([http.close(), secure?.close()]).then((results) => {
        const failures = results.filter((result) => result.status === 'rejected');
        if (failures.length) throw new AggregateError(failures.map((result) => result.reason), 'Ambient gateway cleanup is unconfirmed.');
    }); }
}
