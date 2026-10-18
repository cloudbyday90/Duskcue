// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomBytes } from 'node:crypto';
import { appendFile, lstat, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ownedDocker } from '../qualification/owned.mjs';
import { runBounded } from '../qualification/commands.mjs';
import { STOP_CASE, exactPassed } from '../qualification/runtime.mjs';
import { validateProgressiveHttpResult } from '../qualification/progressive-http.mjs';
import { validatePlaybackRecoveryResult } from '../qualification/recovery-http.mjs';
import { validatePlaybackDirectResult } from '../qualification/direct-http.mjs';
import { validatePacingUnitProof } from '../qualification/pacing-unit-proof.mjs';
import { evaluateLinuxHeadroom, readLinuxSample, readLocalEngine } from './linux-engine-headroom.mjs';
import { runCompilerCommand } from './linux-compile-command.mjs';
import { createLinuxDockerProbe } from './linux-docker-probe.mjs';
import { watchLinuxReserve } from './linux-reserve-watchdog.mjs';
import { copyRecoverySnapshot, owningRecoveryDirectory, parseRecoveryInvocation, recoverySources, validateRecoveryImage } from './playback-recovery-inputs.mjs';
import { directSources } from './direct-inputs.mjs';
import { prepareDirectBrowser, runDirectBrowser } from '../qualification/direct-browser.mjs';
import { readQualification } from '../../clients/web/tests/native/qualification-config.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const memory = 4294967296;
const pids = 256;

async function main() {
    const options = parseRecoveryInvocation(process.argv.slice(2));
    const { nativeRecovery, liveClients, directBrowser } = options;
    const nativeManifest = liveClients ? await readQualification(options.nativeManifestPath) : null;
    const sources = directBrowser ? directSources : recoverySources;
    if (process.platform !== 'win32') throw new Error('Use the owning Windows build memory workflow for this local Docker Desktop qualification.');
    const owner = await owningRecoveryDirectory(workspace, process.env, options.workload);
    const result = { version: 1, kind: 'local-playback-recovery', status: 'in_progress', resourceId: owner.resourceId, startedAt: new Date().toISOString(), limits: { runtime: { memoryBytes: memory, memorySwapBytes: memory, cpus: 2, pids, readOnlyRoot: true, user: '65532:65532', capabilities: 'none', seccomp: 'Docker default' }, postgres: { memoryBytes: memory, memorySwapBytes: memory, cpus: 2, pids }, build: { cargoJobs: 2, deadlineMinutes: 30, logMiB: 32, resourceBounds: 'BuildKit builder/daemon; Windows build policy and live Linux memory reserve admission apply. Dockerfile RUN has no separately claimed hard cgroup ceiling.' } }, cleanup: { passed: false }, limitations: ['This is local Linux HTTP/process qualification of the current working tree. Browser-to-live-server playback and corruption-corpus coverage remain unqualified.'] };
    let ownership;
    let failure;
    let reserve;
    let cleaning = false;
    try {
        const engine = await readLocalEngine();
        result.engine = engine;
        const command = (args, extra = {}) => runBounded('docker.exe', ['--context', engine.context, ...args], { cwd: workspace, timeoutMs: 30000, bytes: 1048576, ...extra, abortSignal: cleaning ? undefined : reserve?.signal });
        const metadata = async (args) => {
            const execution = await command(args, { timeoutMs: 5000 });
            if (execution.code !== 0 || execution.signal) throw new Error('Owned recovery Docker metadata was unavailable.');
            return JSON.parse(execution.stdout);
        };
        const checked = async (args, extra) => {
            const execution = await command(args, extra);
            if (execution.code !== 0 || execution.signal) throw new Error('Recovery image or database preparation failed; inspect its bounded log.');
            return execution;
        };
        ownership = ownedDocker(owner.directory, owner.resourceId, { command, metadata }, { marker: owner.marker, context: engine.context });
        const architecture = engine.architecture === 'x86_64' ? 'amd64' : engine.architecture === 'aarch64' ? 'arm64' : engine.architecture;
        if (!['amd64', 'arm64'].includes(architecture)) throw new Error('Recovery qualification requires a native supported Linux architecture.');
        const base = await metadata(['image', 'inspect', options.image, '--format', '{{json .}}']);
        if (base.Id !== options.image || base.Os !== 'linux' || base.Architecture !== architecture || base.Config?.Labels?.['org.opencontainers.image.title'] !== 'Duskcue') throw new Error('The final immutable local Duskcue runtime must match the current engine architecture.');
        result.baseImage = { id: base.Id, architecture: base.Architecture };
        result.source = await sources(workspace);
        await writeFile(join(owner.directory, 'source-manifest.json'), `${JSON.stringify(result.source, null, 2)}\n`);
        const snapshot = join(owner.directory, 'source');
        await copyRecoverySnapshot(workspace, snapshot, result.source, sources);
        const baseTag = `duskcue-playback-recovery-base:${owner.resourceId}`;
        const runtimeTag = `duskcue-playback-recovery:${owner.resourceId}`;
        await checked(['tag', base.Id, baseTag]);
        if ((await metadata(['image', 'inspect', baseTag, '--format', '{{json .}}'])).Id !== base.Id) throw new Error('Local runtime base tag identity changed.');
        const initial = await readLinuxSample(engine);
        const decision = evaluateLinuxHeadroom(initial, initial);
        if (!decision.allowed) throw new Error(decision.reason);
        result.linuxPreflight = { sample: initial, decision };
        const samplePath = join(owner.directory, 'linux-memory.jsonl');
        const recordSample = (sample) => appendFile(samplePath, `${JSON.stringify(sample)}\n`);
        await recordSample(initial);
        const args = ['--context', engine.context, 'build', '--progress=plain', `--platform=linux/${architecture}`, '--target=runtime', '-f', join(snapshot, 'docker/qualification/Dockerfile.recovery'), '--build-arg', `RUNTIME_IMAGE=${baseTag}`, '--build-arg', `RUNTIME_BASE_ID=${base.Id}`, '--build-arg', `SOURCE_SHA256=${result.source.sha256}`, '--build-arg', `NATIVE_RECOVERY_MODE=${nativeRecovery ? 'paced' : 'none'}`, '-t', runtimeTag, snapshot];
        const probe = nativeRecovery ? await createLinuxDockerProbe(owner, engine, initial) : null;
        try {
            const monitored = probe?.initial || initial;
            const admitted = evaluateLinuxHeadroom(monitored, initial);
            if (!admitted.allowed) throw new Error(admitted.reason);
            if (probe) { result.compilerMemoryProbe = { id: probe.id, image: probe.image, initial: monitored }; await recordSample(monitored); }
            result.compilation = await runCompilerCommand(workspace, args, owner, engine, monitored, recordSample, probe?.read);
        } finally { if (probe) result.compilerProbeCleanup = { removed: await probe.cleanup(), passed: true }; }
        if ((await sources(workspace)).sha256 !== result.source.sha256 || (await sources(snapshot)).sha256 !== result.source.sha256) throw new Error('Current recovery sources changed during compilation.');
        result.image = validateRecoveryImage(await metadata(['image', 'inspect', runtimeTag, '--format', '{{json .}}']), { source: result.source.sha256, base: base.Id, architecture, nativeRecovery });
        reserve = await watchLinuxReserve(engine, recordSample);
        result.runtimeLinuxPreflight = reserve.initial;
        await checked(['pull', `--platform=linux/${architecture}`, 'postgres:18'], { log: join(owner.directory, 'postgres-image.log'), timeoutMs: 300000 });
        const pgImage = await metadata(['image', 'inspect', 'postgres:18', '--format', '{{json .}}']);
        if (!/^sha256:[a-f0-9]{64}$/.test(pgImage.Id || '') || pgImage.Os !== 'linux' || pgImage.Architecture !== architecture) throw new Error('PostgreSQL18 runtime platform is invalid.');
        result.postgresImage = { id: pgImage.Id, architecture: pgImage.Architecture };
        const password = randomBytes(24).toString('base64url');
        const database = `duskcue_transcode_test_${owner.resourceId.replaceAll('-', '')}`;
        const databaseUrl = `postgres://duskcue_fixture:${password}@127.0.0.1:5432/${database}`;
        result.postgresLinuxPreflight = await reserve.admit();
        const pgArguments = nativeRecovery ? ['postgres', '-c', 'shared_buffers=16MB', '-c', 'max_connections=16'] : [];
        const pg = await ownership.create(pgImage.Id, pgArguments, { memory, pids, network: 'none', user: 'postgres', readOnly: true,
            tmpfs: { '/tmp': 'rw,nosuid,nodev,noexec,size=536870912,mode=1777', '/var/run/postgresql': 'rw,nosuid,nodev,noexec,size=16777216,mode=1777', '/var/lib/postgresql': 'rw,nosuid,nodev,noexec,size=16777216,mode=1777' },
            environment: { POSTGRES_DB: database, POSTGRES_USER: 'duskcue_fixture', POSTGRES_PASSWORD: password, PGDATA: '/tmp/duskcue-postgres' } });
        await ownership.startDetached(pg);
        if (!(await ownership.inspect(pg)).state.running) throw new Error('The owned PostgreSQL18 fixture exited before readiness.');
        let ready = false;
        const readyDeadline = Date.now() + 30000;
        while (Date.now() < readyDeadline) {
            const probe = await command(['exec', pg, 'pg_isready', '-t', '1', '-h', '127.0.0.1', '-U', 'duskcue_fixture', '-d', database], { timeoutMs: Math.max(1, Math.min(2500, readyDeadline - Date.now())) });
            if (probe.code === 0 && !probe.signal) { ready = true; break; }
            await new Promise((resume) => setTimeout(resume, Math.max(0, Math.min(200, readyDeadline - Date.now()))));
        }
        if (!ready) throw new Error('The isolated disposable PostgreSQL18 fixture did not become ready.');
        if (nativeRecovery) {
            const observed = await checked(['exec', pg, 'psql', '-X', '-A', '-t', '--no-password', '-U', 'duskcue_fixture', '-d', database, '-c', "SELECT json_build_object('sharedBuffersBytes',pg_size_bytes(current_setting('shared_buffers')),'maxConnections',current_setting('max_connections')::int)::text"], { timeoutMs: 5000, bytes: 4096 });
            result.postgresFixtureSettings = JSON.parse(observed.stdout);
            if (result.postgresFixtureSettings.sharedBuffersBytes !== 16777216 || result.postgresFixtureSettings.maxConnections !== 16) throw new Error('The paced disposable database settings do not match the bounded fixture profile.');
        }
        const assets = directBrowser ? await prepareDirectBrowser(workspace, owner.directory) : null;
        result.testLinuxPreflight = await reserve.admit();
        const runtime = await ownership.create(result.image.id, [STOP_CASE, '--exact', '--include-ignored', '--nocapture'], { memory, pids, network: `container:${pg}`, user: '65532:65532', readOnly: true,
            tmpfs: { '/tmp': 'rw,nosuid,nodev,noexec,size=67108864,uid=65532,gid=65532,mode=1770', '/src/.cache/tonight-server-stop': 'rw,nosuid,nodev,size=536870912,uid=65532,gid=65532,mode=1770', '/data': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770', '/cache': 'rw,nosuid,nodev,noexec,size=16777216,uid=65532,gid=65532,mode=1770' },
            environment: { DUSKCUE_TEST_RESOURCE_ID: owner.resourceId, DUSKCUE_DATABASE_URL: databaseUrl, ...(directBrowser ? { DUSKCUE_TEST_DIRECT_BROWSER: 'live' } : {}), ...(liveClients ? { DUSKCUE_TEST_NATIVE_CLIENT: 'live' } : {}), ...(nativeRecovery ? { DUSKCUE_TEST_NATIVE_RECOVERY: 'live' } : {}) } });
        const running = ownership.start(runtime, { log: join(owner.directory, 'playback-stop-contract.log'), bytes: 16777216, captureBytes: 16777216, timeoutMs: liveClients ? 600000 : directBrowser ? 420000 : 300000, redact: [databaseUrl, password] });
        let execution;
        if (directBrowser) {
            const direct = await runDirectBrowser({ workspace, directory: owner.directory, context: engine.context, runtime, resourceId: owner.resourceId, execution: running, ownership, assets, abortSignal: reserve.signal, nativeManifest, nativeRecovery });
            execution = direct.execution;
            result.directBrowser = direct.browser;
            result.limitations = ['Chrome mounts the actual Player component, not the application shell or adapter-node proxy.', 'Physical A/V synchronization and broader format/HDR fidelity remain separate gates.', ...(nativeRecovery ? ['Native recovery uses a paced qualification input and known-keyframe paused seeks; ordinary production performance and arbitrary-playhead copy/remux fidelity remain separate.'] : ['Actual native HLS process-failure recovery requires the paced native mode.']), ...(liveClients ? [] : ['Actual desktop bearer/HLS presentation requires the live-client mode.'])];
            if (nativeManifest) result.nativeArtifact = { identifier: nativeManifest.identifier, executableSha256: nativeManifest.executableSha256 };
        } else execution = await running;
        result.runtime = await ownership.inspect(runtime);
        if (execution.code !== 0 || execution.signal || result.runtime.state.exitCode !== 0 || result.runtime.state.oomKilled || !exactPassed(execution.stdout)) throw new Error('The exact current Linux playback stop/recovery contract did not pass.');
        result.progressiveHttp = validateProgressiveHttpResult(execution.stdout, owner.resourceId);
        result.recovery = validatePlaybackRecoveryResult(execution.stdout, owner.resourceId);
        result.direct = validatePlaybackDirectResult(execution.stdout, owner.resourceId, directBrowser, liveClients, nativeRecovery);
        if (nativeRecovery) {
            const path = join(owner.directory, 'pacing-unit-tests.log');
            await ownership.copyOut(runtime, '/qualification/evidence/pacing-unit-tests.log', path);
            const evidence = await lstat(path);
            if (!evidence.isFile() || evidence.isSymbolicLink() || evidence.size > 65536) throw new Error('Pacing unit evidence is not a bounded regular file.');
            result.pacingUnits = validatePacingUnitProof(await readFile(path, 'utf8'));
        }
        await ownership.copyOut(runtime, '/qualification/fixture.json', join(owner.directory, 'fixture.json'));
        result.fixture = JSON.parse(await readFile(join(owner.directory, 'fixture.json'), 'utf8'));
        await ownership.remove(runtime);
        await ownership.remove(pg);
        if ((await sources(workspace)).sha256 !== result.source.sha256) throw new Error('Current working-tree sources changed during recovery qualification.');
        result.status = 'passed';
    } catch (error) { result.status = 'failed'; result.error = error.message; failure = error; }
    finally {
        cleaning = true;
        try { result.cleanup = { passed: true, removed: await ownership?.cleanup() || [] }; }
        catch (error) { result.status = 'failed'; result.cleanup = { passed: false, error: error.message }; failure ||= error; }
        try { await reserve?.finish(); }
        catch (error) { result.status = 'failed'; result.linuxReserveError = error.message; failure ||= error; }
        result.finishedAt = new Date().toISOString();
        await writeFile(join(owner.directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);
    }
    console.log(JSON.stringify({ status: result.status, result: join(owner.directory, 'result.json') }));
    if (failure) throw failure;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try { await main(); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
}
