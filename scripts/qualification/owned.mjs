// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomUUID } from 'node:crypto';
import { appendFile } from 'node:fs/promises';
import { join } from 'node:path';
import { runBounded } from './commands.mjs';

export async function docker(args, options = {}) {
    return runBounded('docker', args, { timeoutMs: 30000, bytes: 1048576, ...options });
}

export async function metadata(args) {
    const result = await docker(args);
    if (result.code !== 0 || result.signal) throw new Error('Owned Docker metadata was unavailable.');
    return JSON.parse(result.stdout);
}

export function validateContainer(info, expected) {
    const host = info.HostConfig || {};
    if (info.Id !== expected.id || info.Name !== `/${expected.name}` || info.Config?.Labels?.['duskcue.test.resource-id'] !== expected.resourceId
        || host.Privileged || host.Memory !== expected.memory || host.MemorySwap !== expected.memory || host.NanoCpus !== 2000000000
        || Number(host.PidsLimit) !== expected.pids || !host.SecurityOpt?.includes('no-new-privileges')
        || info.Mounts?.some((mount) => mount.Destination.includes('docker.sock'))
        || host.SecurityOpt.some((option) => /unconfined/.test(option))) throw new Error('Container ownership or resource/security bounds changed.');
    if (expected.readOnly && (host.ReadonlyRootfs !== true || !host.CapDrop?.includes('ALL') || info.Config?.User !== expected.user
        || info.Image !== expected.image || host.NetworkMode !== expected.network || Object.keys(host.PortBindings || {}).length
        || Object.keys(host.Tmpfs || {}).length !== Object.keys(expected.tmpfs).length
        || Object.entries(expected.tmpfs).some(([path, options]) => host.Tmpfs?.[path] !== options)
        || info.Mounts?.some((mount) => mount.Type !== 'tmpfs'))) throw new Error('Read-only qualification isolation changed.');
    return { id: info.Id, name: expected.name, state: { status: info.State?.Status, running: info.State?.Running === true, exitCode: info.State?.ExitCode, oomKilled: info.State?.OOMKilled === true }, limits: { memory: host.Memory, memorySwap: host.MemorySwap, cpus: 2, pids: Number(host.PidsLimit) } };
}

export function ownedDocker(directory, resourceId, transport = { command: docker, metadata }, registration = {}) {
    const containers = new Map();
    const marker = registration.marker || join(directory, 'owned-containers.jsonl');
    const expectedFor = (identity) => containers.get(identity) || [...containers.values()].find((expected) => expected.id === identity);
    async function recover(expected) {
        if (!expected.id) {
            const listed = await transport.command(['ps', '-aq', '--filter', `name=^${expected.name}$`, '--filter', `label=duskcue.test.resource-id=${resourceId}`]);
            const ids = listed.stdout.trim().split(/\s+/).filter(Boolean);
            if (listed.code !== 0 || ids.length > 1 || ids.some((id) => !/^[a-f0-9]{12,64}$/.test(id))) throw new Error('Pending owned container could not be safely recovered.');
            if (!ids.length) return null;
            const info = await transport.metadata(['inspect', ids[0], '--format', '{{json .}}']);
            if (!/^[a-f0-9]{64}$/.test(info.Id)) throw new Error('Recovered container identity is invalid.');
            expected.id = info.Id;
            return validateContainer(info, expected);
        }
        return validateContainer(await transport.metadata(['inspect', expected.id, '--format', '{{json .}}']), expected);
    }
    return {
        async create(imageId, args, { memory = 536870912, pids = 128, environment = {}, network = 'none', user, readOnly = false, tmpfs = {} } = {}) {
            if (!/^sha256:[a-f0-9]{64}$/.test(imageId) || !/^[a-f0-9-]{36}$/.test(resourceId)) throw new Error('Immutable image and workflow identity required.');
            if (readOnly && (!user || !Object.keys(tmpfs).length || Object.entries(tmpfs).some(([path, options]) => !/^\/[A-Za-z0-9_./-]+$/.test(path)
                || !/^rw,nosuid,nodev,(?:noexec,)?size=[0-9]+(?:,uid=[0-9]+,gid=[0-9]+)?(?:,mode=[0-7]{3,4})?$/.test(options)))) throw new Error('Read-only qualification requires an explicit user and bounded safe tmpfs mounts.');
            const name = `duskcue-playback-${randomUUID()}`;
            const expected = { id: null, name, resourceId, memory, pids, readOnly, user, network, image: imageId, tmpfs };
            containers.set(name, expected);
            const command = ['create', '--pull=never', '--name', name, '--label', `duskcue.test.resource-id=${resourceId}`, '--memory', String(memory), '--memory-swap', String(memory), '--cpus=2', '--pids-limit', String(pids), '--cap-drop=ALL', '--security-opt=no-new-privileges', '--network', network];
            if (user) command.push('--user', user);
            if (readOnly) command.push('--read-only');
            for (const [path, options] of Object.entries(tmpfs)) command.push('--tmpfs', `${path}:${options}`);
            for (const [key, value] of Object.entries(environment)) command.push('--env', `${key}=${value}`);
            command.push(imageId, ...args);
            await appendFile(marker, `${JSON.stringify({ resourceId, name, ...(registration.context ? { context: registration.context } : {}) })}\n`);
            const result = await transport.command(command);
            const id = result.stdout.trim();
            if (result.code !== 0 || !/^[a-f0-9]{64}$/.test(id)) throw new Error('Owned container could not be created.');
            expected.id = id;
            await this.inspect(id);
            return id;
        },
        async inspect(id) {
            const expected = expectedFor(id);
            if (!expected) throw new Error('Container is not owned by this driver.');
            const observed = await recover(expected);
            if (!observed) throw new Error('Owned container is absent.');
            return observed;
        },
        async start(id, options = {}) {
            await this.inspect(id);
            return transport.command(['start', '--attach', id], { timeoutMs: 300000, bytes: 16777216, ...options });
        },
        async startDetached(id) {
            await this.inspect(id);
            const result = await transport.command(['start', id]);
            if (result.code !== 0) throw new Error('Owned database container could not start.');
        },
        async copyOut(id, source, destination, { requireSuccess = true } = {}) {
            const current = await this.inspect(id);
            if (current.state.status !== 'exited' || current.state.running || requireSuccess && (current.state.exitCode !== 0 || current.state.oomKilled)) throw new Error('Only confirmed stopped successful artifacts may be exported.');
            const result = await transport.command(['cp', `${id}:${source}`, destination], { timeoutMs: 120000 });
            if (result.code !== 0) throw new Error('Owned qualification output could not be copied.');
        },
        async copyIn(id, source, destination) {
            const current = await this.inspect(id);
            if (current.state.running) throw new Error('Fixtures must enter the created overlay before start.');
            const result = await transport.command(['cp', source, `${id}:${destination}`], { timeoutMs: 120000 });
            if (result.code !== 0) throw new Error('Owned fixture could not enter the Linux overlay.');
        },
        async remove(id) {
            const expected = expectedFor(id);
            if (!expected) throw new Error('Cleanup target is not owned.');
            const observed = await recover(expected);
            if (!observed) { containers.delete(expected.name); return null; }
            const result = await transport.command(['rm', '--force', '--volumes', observed.id]);
            if (result.code !== 0) throw new Error('Owned container cleanup failed.');
            containers.delete(expected.name);
            return observed.id;
        },
        async cleanup() {
            const removed = [];
            for (const expected of [...containers.values()]) { const id = await this.remove(expected.id || expected.name); if (id) removed.push(id); }
            return removed;
        },
    };
}
