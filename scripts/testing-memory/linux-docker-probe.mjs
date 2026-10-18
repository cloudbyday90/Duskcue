// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { docker, ownedDocker } from '../qualification/owned.mjs';
import { cleanupOwnedContainers } from './owned-containers.mjs';
import { MetadataReadError, boundedMetadata, evaluateLinuxHeadroom, parseLinuxSample, readMatchingEngine } from './linux-engine-headroom.mjs';

export async function readDockerLinuxSample(engine, baseline, probe, { readMetadata = boundedMetadata, now = Date.now } = {}) {
    const checks = await Promise.allSettled([readMatchingEngine(engine, readMetadata), probe.inspect()]);
    const errors = checks.filter((check) => check.status === 'rejected').map((check) => check.reason);
    if (checks[1].status === 'fulfilled' && (!checks[1].value.state.running || checks[1].value.state.oomKilled)) errors.push(new Error('The owned Linux memory probe is not running.'));
    if (errors.length) throw errors.find((error) => !(error instanceof MetadataReadError)) || errors[0];
    const observedAt = now();
    const output = await readMetadata('docker.exe', ['--context', engine.context, 'exec', '--user', '65532:65532', probe.id, 'cat', '/proc/meminfo', '/proc/sys/kernel/random/boot_id', '/proc/version']);
    const sample = { ...parseLinuxSample(output, engine, observedAt), transport: 'owned-docker-exec', probeId: probe.id };
    const decision = evaluateLinuxHeadroom(sample, baseline, { nowMs: now(), preflight: false });
    if (sample.totalBytes !== baseline.totalBytes || !decision.allowed) throw new Error(decision.reason || 'The Linux memory probe no longer matches the validated WSL memory total.');
    return sample;
}

export async function createLinuxDockerProbe(owner, engine, baseline, { readMetadata = boundedMetadata } = {}) {
    const available = await readMetadata('docker.exe', ['--context', engine.context, 'image', 'ls', '--no-trunc', '--filter', 'reference=duskcue:local', '--format', '{{.ID}}']);
    if (!available) return null;
    if (!/^sha256:[a-f0-9]{64}$/.test(available)) throw new Error('The existing local probe image identity is ambiguous.');
    const command = (args, options) => docker(['--context', engine.context, ...args], { timeoutMs: 5000, bytes: 65536, ...options });
    const metadata = async (args) => JSON.parse(await readMetadata('docker.exe', ['--context', engine.context, ...args]));
    const ownership = ownedDocker(owner.directory, owner.resourceId, { command, metadata }, { marker: owner.marker, context: engine.context });
    try {
        const image = await metadata(['image', 'inspect', available, '--format', '{{json .}}']);
        const architecture = engine.architecture === 'x86_64' ? 'amd64' : engine.architecture === 'aarch64' ? 'arm64' : engine.architecture;
        if (!/^sha256:[a-f0-9]{64}$/.test(image.Id || '') || image.Os !== 'linux' || image.Architecture !== architecture || image.Config?.Labels?.['org.opencontainers.image.title'] !== 'Duskcue') throw new Error('The Linux memory probe requires an existing immutable native Duskcue image.');
        const id = await ownership.create(image.Id, ['/bin/sleep', 'infinity'], { memory: 67108864, pids: 32, network: 'none', user: '65532:65532', readOnly: true, noHealthcheck: true,
            tmpfs: { '/data': 'rw,nosuid,nodev,noexec,size=1048576,uid=65532,gid=65532', '/cache': 'rw,nosuid,nodev,noexec,size=1048576,uid=65532,gid=65532' } });
        await ownership.startDetached(id);
        const probe = { id, inspect: () => ownership.inspect(id) };
        const read = () => readDockerLinuxSample(engine, baseline, probe, { readMetadata });
        const initial = await read();
        return { id, image: image.Id, initial, read, cleanup: () => cleanupOwnedContainers(owner.marker, owner.resourceId) };
    } catch (error) {
        await cleanupOwnedContainers(owner.marker, owner.resourceId);
        throw error;
    }
}
