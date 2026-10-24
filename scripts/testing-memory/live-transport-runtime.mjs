// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomBytes } from 'node:crypto';
import { join } from 'node:path';
import { validateTransportProof } from './live-transport-proof.mjs';

export async function qualifyTransportRuntime({ owner, ownership, checked, metadata, reserve, image }) {
    const memory = 1073741824;
    const postgres = await metadata(['image', 'inspect', 'postgres:18', '--format', '{{json .}}']);
    if (!/^sha256:[a-f0-9]{64}$/.test(postgres.Id) || postgres.Os !== 'linux' || postgres.Architecture !== image.Architecture) throw new Error('A matching local immutable PostgreSQL18 image is required.');
    const password = randomBytes(24).toString('base64url');
    const database = `duskcue_live_transport_test_${owner.resourceId.replaceAll('-', '')}`;
    const databaseUrl = `postgres://duskcue_fixture:${password}@127.0.0.1:5432/${database}`;
    await reserve.admit();
    const pg = await ownership.create(postgres.Id, ['postgres', '-c', 'shared_buffers=16MB', '-c', 'max_connections=16'], { memory, pids: 128, network: 'none', user: 'postgres', readOnly: true,
        tmpfs: { '/tmp': 'rw,nosuid,nodev,noexec,size=268435456,mode=1777', '/var/run/postgresql': 'rw,nosuid,nodev,noexec,size=16777216,mode=1777', '/var/lib/postgresql': 'rw,nosuid,nodev,noexec,size=16777216,mode=1777' },
        environment: { POSTGRES_DB: database, POSTGRES_USER: 'duskcue_fixture', POSTGRES_PASSWORD: password, PGDATA: '/tmp/duskcue-postgres' } });
    await ownership.startDetached(pg);
    const deadline = Date.now() + 30000;
    let ready = false;
    while (Date.now() < deadline) {
        try { await checked(['exec', pg, 'pg_isready', '-t', '1', '-h', '127.0.0.1', '-U', 'duskcue_fixture', '-d', database], { timeoutMs: 2500 }); ready = true; break; }
        catch { await new Promise((resolve) => setTimeout(resolve, 200)); }
    }
    if (!ready) throw new Error('Disposable PostgreSQL18 readiness failed.');
    const settings = await checked(['exec', pg, 'psql', '-X', '-A', '-t', '--no-password', '-U', 'duskcue_fixture', '-d', database, '-c', "SELECT json_build_object('sharedBuffersBytes',pg_size_bytes(current_setting('shared_buffers')),'maxConnections',current_setting('max_connections')::int)::text"], { bytes: 4096 });
    const observed = JSON.parse(settings.stdout);
    if (observed.sharedBuffersBytes !== 16777216 || observed.maxConnections !== 16) throw new Error('Database fixture bounds changed.');
    await reserve.admit();
    const runtime = await ownership.create(image.Id, ['supervised_mpegts_transport_contract', '--exact', '--include-ignored', '--nocapture'], { memory, pids: 128, network: `container:${pg}`, user: '65532:65532', readOnly: true,
        tmpfs: { '/tmp': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770', '/fixtures': 'rw,nosuid,nodev,noexec,size=67108864,uid=65532,gid=65532,mode=1770', '/data': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770', '/cache': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770' },
        environment: { DUSKCUE_TEST_RESOURCE_ID: owner.resourceId, DUSKCUE_DATABASE_URL: databaseUrl, DUSKCUE_LIVE_TRANSPORT_TESTS: 'disposable', HTTP_PROXY: 'http://127.0.0.1:1', HTTPS_PROXY: 'http://127.0.0.1:1', ALL_PROXY: 'http://127.0.0.1:1', NO_PROXY: '', no_proxy: '' } });
    const execution = await ownership.start(runtime, { log: join(owner.directory, 'transport.log'), timeoutMs: 360000, bytes: 1048576, captureBytes: 1048576, redact: [databaseUrl, password], abortSignal: reserve.signal });
    const state = await ownership.inspect(runtime);
    if (execution.code !== 0 || execution.signal || state.state.exitCode !== 0 || state.state.oomKilled) throw new Error('The exact managed transport contract failed; inspect transport.log.');
    const proof = validateTransportProof(execution.stdout, owner.resourceId);
    await ownership.remove(runtime); await ownership.remove(pg);
    return { proof, runtime: state, postgresImage: postgres.Id, postgresSettings: observed };
}
