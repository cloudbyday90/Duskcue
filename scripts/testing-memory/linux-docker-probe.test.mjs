// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { MetadataReadError, parseLinuxSample } from './linux-engine-headroom.mjs';
import { createLinuxDockerProbe, readDockerLinuxSample } from './linux-docker-probe.mjs';
import { validateContainer } from '../qualification/owned.mjs';

const engine = { context: 'desktop-linux', engineId: 'owned-engine', kernel: '5.15.167.4-microsoft-standard-WSL2' };
const info = { id: engine.engineId, kernel: engine.kernel, osType: 'linux', name: 'docker-desktop', operatingSystem: 'Docker Desktop', architecture: 'x86_64' };
const memory = `MemTotal: 8388608 kB\nMemAvailable: 6291456 kB\naaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee\nLinux version ${engine.kernel} owned-fixture\n`;
const baseline = parseLinuxSample(memory, engine, 100000);
const probe = { id: 'a'.repeat(64), inspect: async () => ({ state: { running: true, oomKilled: false } }) };

test('initial bootstrap can retain WSL without any existing image or owned-container mutation', async () => {
    const calls = [];
    assert.equal(await createLinuxDockerProbe(null, engine, baseline, { readMetadata: async (executable, args) => { calls.push([executable, args]); return ''; } }), null);
    assert.deepEqual(calls, [['docker.exe', ['--context', engine.context, 'image', 'ls', '--no-trunc', '--filter', 'reference=duskcue:local', '--format', '{{.ID}}']]]);
    for (const available of ['untrusted', `sha256:${'a'.repeat(64)}\nsha256:${'b'.repeat(64)}`]) await assert.rejects(createLinuxDockerProbe(null, engine, baseline, { readMetadata: async () => available }), /ambiguous/);
});

test('owned Docker transport reads actual proc files only after engine and container validation', async () => {
    let inspected = false;
    const calls = [];
    const sample = await readDockerLinuxSample(engine, baseline, { ...probe, inspect: async () => { inspected = true; return probe.inspect(); } }, {
        now: () => 100000,
        readMetadata: async (executable, args) => {
            calls.push([executable, args]);
            if (args[2] === 'info') return JSON.stringify(info);
            assert.equal(inspected, true);
            assert.deepEqual(args, ['--context', engine.context, 'exec', '--user', '65532:65532', probe.id, 'cat', '/proc/meminfo', '/proc/sys/kernel/random/boot_id', '/proc/version']);
            return memory;
        },
    });
    assert.equal(calls.length, 2);
    assert.equal(sample.probeId, probe.id);
    assert.equal(sample.transport, 'owned-docker-exec');
    assert.equal(sample.availableBytes, baseline.availableBytes);
});

test('probe refuses changed boot, total, engine, exhausted reserve and stale samples', async () => {
    for (const [text, current, times] of [
        [memory.replace('aaaaaaaa-', 'bbbbbbbb-'), info, [100000, 100000]],
        [memory.replace('8388608', '8388609'), info, [100000, 100000]],
        [memory, { ...info, id: 'foreign-engine' }, [100000, 100000]],
        [memory.replace('6291456', '1024'), info, [100000, 100000]],
        [memory, info, [100000, 110001]],
    ]) {
        let clock = 0;
        await assert.rejects(readDockerLinuxSample(engine, baseline, probe, { now: () => times[Math.min(clock++, 1)], readMetadata: async (_executable, args) => args[2] === 'info' ? JSON.stringify(current) : text }));
    }
});

test('probe failure is fatal and cannot be masked by concurrent transient engine metadata failure', async () => {
    let reads = 0;
    for (const observed of [{ state: { running: false } }, { state: { running: true, oomKilled: true } }]) {
        await assert.rejects(readDockerLinuxSample(engine, baseline, { ...probe, inspect: async () => observed }, { readMetadata: async () => { reads++; return JSON.stringify(info); } }), /not running/);
        await assert.rejects(readDockerLinuxSample(engine, baseline, { ...probe, inspect: async () => observed }, { readMetadata: async () => { throw new MetadataReadError('docker.exe', { code: 'ETIMEDOUT' }); } }), /not running/);
    }
    assert.equal(reads, 2);
    await assert.rejects(readDockerLinuxSample(engine, baseline, { ...probe, inspect: async () => { throw new Error('Container ownership changed.'); } }, { readMetadata: async () => { throw new MetadataReadError('docker.exe', { code: 'ETIMEDOUT' }); } }), /ownership changed/);
});

test('sleep-only probes require the inherited application healthcheck to remain disabled', () => {
    const expected = { id: probe.id, name: 'duskcue-playback-fixture', resourceId: 'owner', memory: 67108864, pids: 32, noHealthcheck: true };
    const container = { Id: expected.id, Name: `/${expected.name}`, Config: { Labels: { 'duskcue.test.resource-id': 'owner' }, Healthcheck: { Test: ['NONE'] } }, HostConfig: { Memory: expected.memory, MemorySwap: expected.memory, NanoCpus: 2000000000, PidsLimit: expected.pids, SecurityOpt: ['no-new-privileges'] }, Mounts: [], State: { Running: true } };
    validateContainer(container, expected);
    for (const test of [undefined, [], ['CMD', 'curl'], ['NONE', 'unexpected']]) {
        const changed = structuredClone(container);
        changed.Config.Healthcheck.Test = test;
        assert.throws(() => validateContainer(changed, expected), /healthcheck changed/);
    }
});
