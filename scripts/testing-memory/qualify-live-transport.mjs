// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { appendFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ownedDocker } from '../qualification/owned.mjs';
import { runBounded } from '../qualification/commands.mjs';
import { ownedWorkflowEvidence } from './workflow-evidence.mjs';
import { recoverySources, copyRecoverySnapshot } from './playback-recovery-inputs.mjs';
import { readLocalEngine, readLinuxSample, evaluateLinuxHeadroom } from './linux-engine-headroom.mjs';
import { createLinuxDockerProbe } from './linux-docker-probe.mjs';
import { runCompilerCommand } from './linux-compile-command.mjs';
import { watchLinuxReserve } from './linux-reserve-watchdog.mjs';
import { qualifyTransportRuntime } from './live-transport-runtime.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
const args = process.argv.slice(2);
if (args.length !== 2 || args[0] !== '--image' || !/^sha256:[a-f0-9]{64}$/.test(args[1])) throw new Error('Provide --image <immutable local Duskcue runtime SHA256>.');
const owner = await ownedWorkflowEvidence(workspace, process.env, 'iptv-transport-qualify');
const result = { version: 1, kind: 'iptv-transport-v1', status: 'running', resourceId: owner.resourceId, startedAt: new Date().toISOString() };
let ownership;
let reserve;
let failure;
let cleaning = false;
let command;
let metadata;
let baseTag;
let runtimeTag;
try {
    const engine = await readLocalEngine(); result.engine = engine;
    command = (argv, options = {}) => runBounded('docker.exe', ['--context', engine.context, ...argv], { cwd: workspace, timeoutMs: 30000, bytes: 1048576, ...(reserve && !cleaning ? { abortSignal: reserve.signal } : {}), ...options });
    metadata = async (argv) => {
        const r = await command(argv, { timeoutMs: 5000 });
        if (r.code !== 0 || r.signal) throw new Error('Owned transport Docker metadata unavailable.');
        return JSON.parse(r.stdout);
    };
    const checked = async (argv, options) => { const r = await command(argv, options); if (r.code !== 0 || r.signal) throw new Error('Owned transport preparation failed.'); return r; };
    ownership = ownedDocker(owner.directory, owner.resourceId, { command, metadata }, { marker: owner.marker, context: engine.context });
    const base = await metadata(['image', 'inspect', args[1], '--format', '{{json .}}']);
    const architecture = engine.architecture === 'x86_64' ? 'amd64' : engine.architecture === 'aarch64' ? 'arm64' : '';
    if (base.Id !== args[1] || base.Os !== 'linux' || base.Architecture !== architecture || base.Config?.Labels?.['org.opencontainers.image.title'] !== 'Duskcue') throw new Error('An immutable native Duskcue base is required.');
    result.baseImage = base.Id;
    result.source = await recoverySources(workspace);
    const snapshot = join(owner.directory, 'source');
    await copyRecoverySnapshot(workspace, snapshot, result.source);
    await writeFile(join(owner.directory, 'source-manifest.json'), `${JSON.stringify(result.source, null, 2)}\n`);
    const initial = await readLinuxSample(engine);
    const decision = evaluateLinuxHeadroom(initial, initial); result.linuxAdmission = { initial, decision };
    if (!decision.allowed) throw new Error(decision.reason);
    const record = (sample) => appendFile(join(owner.directory, 'linux-memory.jsonl'), `${JSON.stringify(sample)}\n`);
    await record(initial);
    baseTag = `duskcue-live-transport-base:${owner.resourceId}`;
    runtimeTag = `duskcue-live-transport:${owner.resourceId}`;
    await checked(['tag', base.Id, baseTag]);
    const probe = await createLinuxDockerProbe(owner, engine, initial);
    try {
        const monitored = probe?.initial || initial;
        if (!evaluateLinuxHeadroom(monitored, initial).allowed) throw new Error('Linux compiler reserve unavailable.');
        result.compilation = await runCompilerCommand(workspace, ['--context', engine.context, 'build', '--progress=plain', `--platform=linux/${architecture}`, '--target=runtime', '-f', join(snapshot, 'docker/qualification/Dockerfile.live-transport'), '--build-arg', `RUNTIME_IMAGE=${baseTag}`, '--build-arg', `RUNTIME_BASE_ID=${base.Id}`, '--build-arg', `SOURCE_SHA256=${result.source.sha256}`, '-t', runtimeTag, snapshot], owner, engine, monitored, record, probe?.read);
    } finally { if (probe) { await probe.cleanup(); result.compilerProbeRemoved = true; } }
    if ((await recoverySources(workspace)).sha256 !== result.source.sha256 || (await recoverySources(snapshot)).sha256 !== result.source.sha256) throw new Error('Transport sources changed during compilation.');
    const image = await metadata(['image', 'inspect', runtimeTag, '--format', '{{json .}}']);
    if (image.Os !== 'linux' || image.Architecture !== architecture || image.Config?.Labels?.['duskcue.qualification.kind'] !== 'iptv-transport-v1'
        || image.Config.Labels['duskcue.qualification.source-sha256'] !== result.source.sha256 || image.Config.Labels['duskcue.qualification.runtime-base-id'] !== base.Id
        || image.Config.User !== '65532:65532' || JSON.stringify(image.Config.Entrypoint) !== JSON.stringify(['/qualification/bin/live-transport-contract'])) throw new Error('Transport image binding is invalid.');
    result.image = { id: image.Id, sourceSha256: result.source.sha256, base: base.Id };
    reserve = await watchLinuxReserve(engine, record);
    result.runtime = await qualifyTransportRuntime({ owner, ownership, checked, metadata, reserve, image });
    if ((await recoverySources(workspace)).sha256 !== result.source.sha256) throw new Error('Transport sources changed during runtime.');
    result.status = 'passed';
} catch (error) { result.status = 'failed'; result.error = error.message; failure = error; }
finally {
    cleaning = true;
    try { result.cleanup = { removed: await ownership?.cleanup() || [], passed: true }; }
    catch (error) { result.cleanup = { passed: false, error: error.message }; result.status = 'failed'; failure ||= error; }
    try { await reserve?.finish(); } catch (error) { result.reserveError = error.message; result.status = 'failed'; failure ||= error; }
    for (const [tag, identity] of [[runtimeTag, result.image?.id], [baseTag, result.baseImage]]) {
        if (!tag || !identity || !metadata) continue;
        try { if ((await metadata(['image', 'inspect', tag, '--format', '{{json .}}'])).Id !== identity) throw new Error('Owned image tag identity changed.'); const r = await command(['image', 'rm', tag]); if (r.code !== 0) throw new Error('Owned image tag removal failed.'); }
        catch (error) { result.status = 'failed'; result.tagCleanupError = error.message; failure ||= error; }
    }
    result.finishedAt = new Date().toISOString();
    await writeFile(join(owner.directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);
}
console.log(JSON.stringify({ status: result.status, result: join(owner.directory, 'result.json') }));
if (failure) { console.error(failure.message); process.exitCode = 1; }
