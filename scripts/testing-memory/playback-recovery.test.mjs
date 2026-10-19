// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { PRESETS } from './presets.mjs';
import { parseRecoveryArguments, parseRecoveryInvocation, validateRecoveryImage } from './playback-recovery-inputs.mjs';
import { validatePlaybackRecoveryResult } from '../qualification/recovery-http.mjs';
import { validateContainer } from '../qualification/owned.mjs';
import { runBounded } from '../qualification/commands.mjs';
import { watchLinuxReserve } from './linux-reserve-watchdog.mjs';

const resourceId = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';
const image = `sha256:${'a'.repeat(64)}`;

function evidence() {
    return { version: 1, executed: true, passed: true, resource_id: resourceId, source_seconds: 600, source_bytes: 1000000,
        real_process_stall: true, healthy_hint_did_not_restart: true, generation_replay_preserved_position_budget: true, seek_and_stop_serialized_with_heartbeat: true,
        recovery_kept_original_profile: true, cleanup_passed: true, fixture_children_confirmed_exited: true, browser_to_live_server: 'unqualified',
        encoder_identities: [1, 2, 3].map((pid) => ({ pid, parent_pid: 1, started_ticks: 100 + pid })), decoded_stream_relative_timestamps: [0.08, 0.08, 0.08] };
}

const marker = (result) => `DUSKCUE_PLAYBACK_RECOVERY=${JSON.stringify(result)}\n`;

test('local recovery is admitted only through build policy and an immutable final image', () => {
    assert.equal(PRESETS['playback-recovery-qualify'].policy, 'build');
    assert.equal(parseRecoveryArguments(['--image', image]).image, image);
    for (const args of [[], ['--image', 'duskcue:local'], ['--image', image, '--jobs=12']]) assert.throws(() => parseRecoveryArguments(args));
});

test('recovery admission rejects zero execution, incomplete cleanup and fabricated generation proof', () => {
    assert.deepEqual(validatePlaybackRecoveryResult(marker(evidence()), resourceId), evidence());
    for (const stdout of ['', marker(evidence()) + marker(evidence()), marker({ ...evidence(), executed: false }), marker({ ...evidence(), resource_id: '00000000-0000-4000-8000-000000000000' }), marker({ ...evidence(), cleanup_passed: false }), marker({ ...evidence(), source_bytes: 33554433 }), marker({ ...evidence(), source_seconds: 8 }), marker({ ...evidence(), encoder_identities: evidence().encoder_identities.slice(1) }), marker({ ...evidence(), decoded_stream_relative_timestamps: [10, 10, 30] })]) assert.throws(() => validatePlaybackRecoveryResult(stdout, resourceId));
});

test('recovery image must preserve actual source, base and direct test executable identity', () => {
    const expected = { source: 'b'.repeat(64), base: image, architecture: 'amd64' };
    const info = { Id: image, Os: 'linux', Architecture: 'amd64', Config: { User: '65532:65532', Entrypoint: ['/qualification/bin/playback-stop-contract'], Labels: { 'duskcue.qualification.kind': 'local-playback-recovery-v1', 'duskcue.qualification.source-sha256': expected.source, 'duskcue.qualification.runtime-base-id': image, 'duskcue.qualification.native-recovery': 'none', 'duskcue.qualification.format-corpus': 'none' } } };
    assert.equal(validateRecoveryImage(info, expected).id, image);
    for (const mutate of [(entry) => { entry.Config.User = 'root'; }, (entry) => { entry.Config.Entrypoint = ['/usr/local/bin/duskcue-entrypoint']; }, (entry) => { entry.Architecture = 'arm64'; }, (entry) => { entry.Config.Labels['duskcue.qualification.source-sha256'] = 'c'.repeat(64); }]) {
        const changed = structuredClone(info); mutate(changed); assert.throws(() => validateRecoveryImage(changed, expected));
    }
    assert.throws(() => validateRecoveryImage(info, { ...expected, nativeRecovery: true }));
    const paced = structuredClone(info);
    paced.Config.Labels['duskcue.qualification.native-recovery'] = 'paced';
    assert.equal(validateRecoveryImage(paced, { ...expected, nativeRecovery: true }).nativeRecoveryMode, 'paced');
    assert.throws(() => validateRecoveryImage(paced, expected));
});

test('paced native qualification requires its explicit mode, owning native policy and fresh manifest', () => {
    const input = ['--native-recovery', '--native-manifest', 'owned/manifest.json', '--image', image];
    const mode = parseRecoveryInvocation(input);
    assert.equal(mode.nativeRecovery, true);
    assert.equal(mode.liveClients, true);
    assert.equal(mode.directBrowser, true);
    assert.equal(mode.workload, 'playback-native-recovery-qualify');
    assert.equal(PRESETS[mode.workload].policy, 'native');
    assert.equal(PRESETS[mode.workload].heapMiB, 1536);
    assert.equal(parseRecoveryInvocation(['--live-clients', ...input.slice(1)]).nativeRecovery, false);
    assert.equal(parseRecoveryInvocation(['--image', image]).workload, 'playback-recovery-qualify');
    assert.equal(parseRecoveryInvocation(['--direct-browser', '--image', image]).directBrowser, true);
    for (const args of [['--native-recovery', '--image', image], [...input, '--features', 'foreign'], ['--native-recovery', '--native-manifest', 'owned/manifest.json']]) assert.throws(() => parseRecoveryInvocation(args));
});

test('format matrix admission preserves paced native ownership and immutable corpus identity', () => {
    const args = ['--format-corpus', '--native-manifest', 'owned/manifest.json', '--image', image];
    const mode = parseRecoveryInvocation(args);
    assert.equal(mode.formatCorpus, true);
    assert.equal(mode.nativeRecovery, true);
    assert.equal(mode.liveClients, true);
    assert.equal(mode.workload, 'playback-format-qualify');
    assert.equal(PRESETS[mode.workload].policy, 'native');
    assert.equal(parseRecoveryInvocation(['--image', image]).formatCorpus, false);
    assert.throws(() => parseRecoveryInvocation(['--format-corpus', '--image', image]));
    const expected = { source: 'b'.repeat(64), base: image, architecture: 'amd64', nativeRecovery: true, formatCorpus: true };
    const info = { Id: image, Os: 'linux', Architecture: 'amd64', Config: { User: '65532:65532', Entrypoint: ['/qualification/bin/playback-stop-contract'], Labels: {
        'duskcue.qualification.kind': 'local-playback-recovery-v1', 'duskcue.qualification.source-sha256': expected.source,
        'duskcue.qualification.runtime-base-id': image, 'duskcue.qualification.native-recovery': 'paced', 'duskcue.qualification.format-corpus': 'reference',
    } } };
    assert.equal(validateRecoveryImage(info, expected).id, image);
    assert.throws(() => validateRecoveryImage(info, { ...expected, formatCorpus: false }));
    delete info.Config.Labels['duskcue.qualification.format-corpus'];
    assert.throws(() => validateRecoveryImage(info, expected));
});

test('read-only owned runtime rejects root, writable roots, foreign namespaces and persistent mounts', () => {
    const expected = { id: 'a'.repeat(64), image, name: `duskcue-playback-${resourceId}`, resourceId, memory: 4294967296, pids: 256, readOnly: true, user: '65532:65532', network: 'container:owned-postgres', tmpfs: { '/tmp': 'rw,nosuid,nodev,size=67108864' } };
    const info = { Id: expected.id, Image: image, Name: `/${expected.name}`, Config: { User: expected.user, Labels: { 'duskcue.test.resource-id': resourceId } }, HostConfig: { Memory: expected.memory, MemorySwap: expected.memory, NanoCpus: 2000000000, PidsLimit: expected.pids, Privileged: false, ReadonlyRootfs: true, CapDrop: ['ALL'], SecurityOpt: ['no-new-privileges'], NetworkMode: expected.network, PortBindings: {}, Tmpfs: expected.tmpfs }, State: { Status: 'created' }, Mounts: [] };
    assert.equal(validateContainer(info, expected).state.status, 'created');
    for (const mutate of [(entry) => { entry.Config.User = 'root'; }, (entry) => { entry.HostConfig.ReadonlyRootfs = false; }, (entry) => { entry.HostConfig.NetworkMode = 'host'; }, (entry) => { entry.HostConfig.PortBindings = { '5432/tcp': [{ HostPort: '5432' }] }; }, (entry) => { entry.HostConfig.CapDrop = []; }, (entry) => { entry.Mounts.push({ Type: 'volume', Destination: '/data' }); }, (entry) => { entry.HostConfig.Tmpfs['/tmp'] = 'rw,size=unbounded'; }]) {
        const changed = structuredClone(info); mutate(changed); assert.throws(() => validateContainer(changed, expected));
    }
});

test('source routing qualification adds an explicit mode without weakening the conservative format lane', () => {
    const input = ['--source-capabilities', '--native-manifest', 'owned/manifest.json', '--image', image];
    const mode = parseRecoveryInvocation(input);
    assert.equal(mode.sourceCapabilities, true);
    assert.equal(mode.formatCorpus, true);
    assert.equal(mode.nativeRecovery, true);
    assert.equal(mode.workload, 'playback-source-qualify');
    assert.equal(PRESETS[mode.workload].policy, 'native');
    assert.equal(PRESETS[mode.workload].heapMiB, 1536);
    assert.equal(parseRecoveryInvocation(['--format-corpus', ...input.slice(1)]).sourceCapabilities, false);
    assert.throws(() => parseRecoveryInvocation(['--source-capabilities', '--image', image]));
    const expected = { source: 'b'.repeat(64), base: image, architecture: 'amd64', nativeRecovery: true, formatCorpus: true, sourceCapabilities: true };
    const info = { Id: image, Os: 'linux', Architecture: 'amd64', Config: { User: '65532:65532', Entrypoint: ['/qualification/bin/playback-stop-contract'], Labels: {
        'duskcue.qualification.kind': 'local-playback-recovery-v1', 'duskcue.qualification.source-sha256': expected.source,
        'duskcue.qualification.runtime-base-id': image, 'duskcue.qualification.native-recovery': 'paced',
        'duskcue.qualification.format-corpus': 'reference', 'duskcue.qualification.source-capabilities': 'exact',
    } } };
    assert.equal(validateRecoveryImage(info, expected).sourceCapabilitiesMode, 'exact');
    assert.throws(() => validateRecoveryImage(info, { ...expected, sourceCapabilities: false }));
    assert.throws(() => validateRecoveryImage(info, { ...expected, nativeRecovery: false }));
    delete info.Config.Labels['duskcue.qualification.source-capabilities'];
    assert.throws(() => validateRecoveryImage(info, expected));
});

test('Linux reserve loss cancels the attached process and remains a cleanup failure', { timeout: 6000 }, async () => {
    let samples = 0;
    const readSample = async () => ({ timestamp: new Date().toISOString(), context: 'local', engineId: 'owned', kernel: 'test', bootId: 'same-boot', totalBytes: 16 * 1024 ** 3, availableBytes: (++samples === 1 ? 8 : 1) * 1024 ** 3 });
    const reserve = await watchLinuxReserve({}, async () => {}, readSample);
    try {
        await assert.rejects(runBounded(process.execPath, ['--max-old-space-size=16', '-e', 'setInterval(()=>{},1000)'], { abortSignal: reserve.signal, timeoutMs: 5000, bytes: 65536 }), /cancelled by resource admission/);
        assert.equal(reserve.signal.aborted, true);
        await assert.rejects(reserve.finish(), /Linux MemAvailable/);
    } finally { await reserve.finish().catch(() => {}); }
});
