// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, mkdtemp, readFile, rm, rmdir, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parseArguments, within, PRODUCER_FILES } from './linux.mjs';
import { exactPassed, exactTestListed } from './runtime.mjs';
import { REQUIRED_UNIT_PREFIXES, selectedUnitCases } from './cases.mjs';
import { ownedDocker, validateContainer } from './owned.mjs';
import { runBounded } from './commands.mjs';
import { SOURCE_ROOTS, SOURCE_FILES, qualificationSources } from './source.mjs';
import { traceCommand, traceSummary, traceSpecification, TRACE_LIMITS } from './diagnostic.mjs';
import { PRODUCER_CASE, EVENT_CANDIDATE_CASE, REQUIRED_MANAGED_CASES } from './cases.mjs';
import { RUNTIME_ARTIFACTS } from './attest.mjs';
import { architectureSpec, nativeArchitecture, assertNativeArchitecture, validateImageArchitecture, validateRustHost, validateElfHeader } from './architecture.mjs';
import { retainKnownDiagnostic } from './diagnostic-runner.mjs';

function elfHeader(machine) {
    const header = Buffer.alloc(64);
    Buffer.from('7f454c46020101', 'hex').copy(header);
    header.writeUInt16LE(3, 16);
    header.writeUInt16LE(machine, 18);
    header.writeUInt32LE(1, 20);
    header.writeUInt16LE(64, 52);
    return header;
}

test('native qualification binds Node and runner architecture without cross-execution', () => {
    assert.equal(nativeArchitecture('x64', 'X64').platform, 'linux/amd64');
    assert.equal(nativeArchitecture('arm64', 'ARM64').platform, 'linux/arm64');
    for (const [node, runner] of [['x64', 'ARM64'], ['arm64', 'X64'], ['arm', 'ARM'], ['x64', undefined], ['arm64', 'arm64']]) assert.throws(() => nativeArchitecture(node, runner));
    assert.throws(() => architectureSpec('ppc64le'));
    assert.throws(() => assertNativeArchitecture('amd64', 'arm64'));
    assert.throws(() => assertNativeArchitecture('arm64', 'x64'));
    assert.equal(assertNativeArchitecture('arm64', 'arm64').machine, 183);
});

test('native image architecture rejects foreign platforms for both qualification lanes', () => {
    for (const architecture of ['amd64', 'arm64']) {
        validateImageArchitecture({ Os: 'linux', Architecture: architecture }, architecture);
        assert.throws(() => validateImageArchitecture({ Os: 'linux', Architecture: architecture === 'amd64' ? 'arm64' : 'amd64' }, architecture));
        assert.throws(() => validateImageArchitecture({ Os: 'windows', Architecture: architecture }, architecture));
    }
});

test('ELF admission refuses crossed machine identity and malformed bounded headers', () => {
    for (const [architecture, machine, other] of [['amd64', 62, 'arm64'], ['arm64', 183, 'amd64']]) {
        const valid = elfHeader(machine);
        validateElfHeader(valid, architecture);
        assert.throws(() => validateElfHeader(valid, other));
        assert.throws(() => validateElfHeader(valid.subarray(0, 63), architecture));
        assert.throws(() => validateElfHeader(Buffer.concat([valid, Buffer.alloc(1)]), architecture));
        for (const offset of [0, 4, 5, 6, 16, 18, 20, 52]) {
            const invalid = Buffer.from(valid);
            invalid[offset] = 255;
            assert.throws(() => validateElfHeader(invalid, architecture));
        }
    }
});

test('compiled toolchain host must match the same native musl architecture', () => {
    const amd64 = 'rustc 1.96.0\nhost: x86_64-unknown-linux-musl\n';
    const arm64 = 'rustc 1.96.0\nhost: aarch64-unknown-linux-musl\n';
    validateRustHost(amd64, 'amd64');
    validateRustHost(arm64, 'arm64');
    assert.throws(() => validateRustHost(amd64, 'arm64'));
    assert.throws(() => validateRustHost(arm64, 'amd64'));
    assert.throws(() => validateRustHost('host: aarch64-unknown-linux-gnu\n', 'arm64'));
    assert.throws(() => validateRustHost('', 'amd64'));
});

test('producer diagnostic preserves the exact selector and never becomes qualification success', () => {
    const command = traceCommand(RUNTIME_ARTIFACTS.libTest, PRODUCER_CASE);
    assert.equal(command.executable, '/usr/bin/strace');
    assert.deepEqual(command.args.slice(0, 8), ['-f', '-qq', '-s', '0', '-e', 'raw=all', '--', RUNTIME_ARTIFACTS.libTest]);
    assert.ok(command.args.includes('--include-ignored'));
    assert.throws(() => traceCommand('/foreign/test', PRODUCER_CASE));
    assert.throws(() => traceCommand(RUNTIME_ARTIFACTS.libTest, 'another_case'));
    assert.equal(TRACE_LIMITS.timeoutMs, 90000);
    assert.equal(TRACE_LIMITS.bytes, 16777216);
    const denied = traceSummary({ code: 101, signal: null, bytes: 20, stderr: '+++ killed by SIGSYS +++' });
    assert.equal(denied.observedSigsys, true);
    assert.equal(denied.countsTowardQualification, false);
    const refused = traceSummary({ code: 1, signal: null, bytes: 20, stderr: 'strace: ptrace(PTRACE_TRACEME): Operation not permitted' });
    assert.equal(refused.tracingRefused, true);
    assert.equal(refused.observedSigsys, false);
});

test('EVENT candidate is required and only its exact failed selector gains a separate bounded diagnostic', () => {
    assert.ok(REQUIRED_MANAGED_CASES.includes(EVENT_CANDIDATE_CASE));
    const command = traceCommand(RUNTIME_ARTIFACTS.libTest, EVENT_CANDIDATE_CASE);
    assert.ok(command.args.includes(EVENT_CANDIDATE_CASE));
    assert.deepEqual(traceSpecification(EVENT_CANDIDATE_CASE), { name: 'event', trace: '/qualification/work/event-strace.log', sourceRequired: true });
    assert.equal(traceSpecification(PRODUCER_CASE).sourceRequired, false);
    assert.throws(() => traceSpecification(`${EVENT_CANDIDATE_CASE}suffix`));
    assert.throws(() => traceSpecification(REQUIRED_MANAGED_CASES[0]));
    assert.throws(() => traceCommand(RUNTIME_ARTIFACTS.stopContract, EVENT_CANDIDATE_CASE));
    const summary = traceSummary({ code: 101, signal: null, bytes: 32, stderr: '+++ killed by SIGSYS +++' }, EVENT_CANDIDATE_CASE);
    assert.equal(summary.trace, '/qualification/work/event-strace.log');
    assert.equal(summary.countsTowardQualification, false);
});

test('failed EVENT diagnostic keeps source provenance and cleans only its registered container', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'duskcue-event-diagnostic-'));
    const commit = 'a'.repeat(40);
    const hash = 'b'.repeat(64);
    const calls = [];
    const ownership = {
        async create(image, args) { calls.push(['create', image, args]); return 'owned-event'; },
        async copyIn(id, source, destination) { calls.push(['copyIn', id, source, destination]); },
        async start(id) { return { code: 0, stdout: `DUSKCUE_CASE_DIAGNOSTIC=${JSON.stringify({ test: EVENT_CANDIDATE_CASE, sourceCommit: commit, sourceSha256: hash, countsTowardQualification: false, exitCode: 101, completed: true })}\n` }; },
        async copyOut(id, source, destination, options) { calls.push(['copyOut', id, source, options]); await mkdir(join(destination, '..'), { recursive: true }); await writeFile(destination, 'trace'); },
        async remove(id) { calls.push(['remove', id]); },
    };
    try {
        const result = await retainKnownDiagnostic(ownership, `sha256:${'c'.repeat(64)}`, { DUSKCUE_SOURCE_COMMIT: commit }, directory, hash, EVENT_CANDIDATE_CASE, '/owned/source.mkv');
        assert.equal(result.countsTowardQualification, false);
        assert.equal(result.exitCode, 101);
        assert.equal(result.traceRetained, true);
        assert.equal(result.trace, 'logs/event-strace.log');
        assert.equal(result.traceBytes, 5);
        assert.deepEqual(calls[1], ['copyIn', 'owned-event', '/owned/source.mkv', '/fixtures/source.mkv']);
        assert.deepEqual(calls.at(-1), ['remove', 'owned-event']);
    } finally {
        await rm(join(directory, 'logs/event-strace.log'), { force: true });
        await rmdir(join(directory, 'logs')).catch((error) => { if (error.code !== 'ENOENT') throw error; });
        await rmdir(directory);
    }
});

test('exact inventory and passing checks refuse zero or skipped tests', () => {
    assert.equal(exactTestListed('example: test\n1 test, 0 benchmarks\n', 'example'), true);
    assert.equal(exactTestListed('other: test\n1 test, 0 benchmarks\n', 'example'), false);
    assert.equal(exactTestListed('example: test\n0 tests, 0 benchmarks\n', 'example'), false);
    assert.equal(exactPassed('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;'), true);
    assert.equal(exactPassed('test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured;'), false);
});

test('required unit groups are complete and command selectors remain bounded', () => {
    const inventory = REQUIRED_UNIT_PREFIXES.map((prefix) => `${prefix}case`);
    assert.equal(selectedUnitCases(inventory).length, REQUIRED_UNIT_PREFIXES.length);
    assert.throws(() => selectedUnitCases(inventory.slice(1)));
    assert.throws(() => selectedUnitCases([...inventory, `${REQUIRED_UNIT_PREFIXES[0]}bad --flag`]));
    assert.equal(PRODUCER_FILES.length, 9);
});

test('owned containers require exact immutable identity and physical bounds', () => {
    const expected = { id: 'a'.repeat(64), name: 'duskcue-playback-fixture', resourceId: 'owner', memory: 536870912, pids: 128 };
    const info = { Id: expected.id, Name: `/${expected.name}`, Config: { Labels: { 'duskcue.test.resource-id': 'owner' } }, HostConfig: { Memory: expected.memory, MemorySwap: expected.memory, NanoCpus: 2000000000, PidsLimit: 128, Privileged: false, SecurityOpt: ['no-new-privileges'] }, State: { Status: 'exited', ExitCode: 0, OOMKilled: false }, Mounts: [] };
    assert.equal(validateContainer(info, expected).state.status, 'exited');
    for (const mutate of [
        (entry) => { entry.Id = 'b'.repeat(64); },
        (entry) => { entry.Config.Labels['duskcue.test.resource-id'] = 'foreign'; },
        (entry) => { entry.HostConfig.MemorySwap += 1; },
        (entry) => { entry.HostConfig.Privileged = true; },
        (entry) => { entry.HostConfig.SecurityOpt.push('seccomp=unconfined'); },
        (entry) => { entry.Mounts.push({ Destination: '/var/run/docker.sock' }); },
    ]) { const changed = structuredClone(info); mutate(changed); assert.throws(() => validateContainer(changed, expected)); }
});

test('CLI requires a full commit and owner path containment rejects escapes', () => {
    assert.equal(parseArguments(['--commit', 'a'.repeat(40), '--output', '/tmp/owned']).commit, 'a'.repeat(40));
    assert.throws(() => parseArguments(['--commit', 'main', '--output', '/tmp/owned']));
    assert.equal(within('/tmp/owner', '/tmp/owner/child'), true);
    assert.equal(within('/tmp/owner', '/tmp/elsewhere'), false);
    assert.equal(within('/tmp/owner', '/tmp/owner'), false);
});

test('failed create observation retains exact pending ownership for cleanup', async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-owner-contract-'));
    const resourceId = 'a0000000-0000-0000-0000-000000000000';
    const id = 'b'.repeat(64);
    let name;
    const removed = [];
    const transport = {
        async command(args) {
            if (args[0] === 'create') { name = args[args.indexOf('--name') + 1]; return { code: 1, stdout: 'observation lost' }; }
            if (args[0] === 'ps') { assert.ok(args.includes(`label=duskcue.test.resource-id=${resourceId}`)); return { code: 0, stdout: `${id}\n` }; }
            if (args[0] === 'rm') { removed.push(args.at(-1)); return { code: 0, stdout: '' }; }
            throw new Error('unexpected command');
        },
        async metadata() { return { Id: id, Name: `/${name}`, Config: { Labels: { 'duskcue.test.resource-id': resourceId } }, HostConfig: { Memory: 536870912, MemorySwap: 536870912, NanoCpus: 2000000000, PidsLimit: 128, Privileged: false, SecurityOpt: ['no-new-privileges'] }, State: { Status: 'created', ExitCode: 0 }, Mounts: [] }; },
    };
    try {
        const owner = ownedDocker(root, resourceId, transport);
        await assert.rejects(owner.create(`sha256:${'c'.repeat(64)}`, []));
        assert.deepEqual(await owner.cleanup(), [id]);
        assert.deepEqual(removed, [id]);
    } finally { await rm(root, { recursive: true }); }
});

test('owned Linux group deadline terminates stdout-holding grandchild after leader exit', { skip: process.platform !== 'linux' }, async () => {
    const code = "import{spawn}from'node:child_process';spawn(process.execPath,['--max-old-space-size=16','-e',\"process.on('SIGTERM',()=>{});setInterval(()=>{},1000)\"],{stdio:['ignore',1,2]});process.exit(0)";
    const started = Date.now();
    await assert.rejects(runBounded(process.execPath, ['--max-old-space-size=16', '--input-type=module', '-e', code], { timeoutMs: 100, bytes: 65536 }));
    assert.ok(Date.now() - started < 5000);
});

test('bounded command log honors drain and completes without losing bytes', { skip: process.platform !== 'linux' }, async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-log-contract-'));
    try {
        const path = join(root, 'output.log');
        const result = await runBounded(process.execPath, ['--max-old-space-size=16', '-e', "process.stdout.end(Buffer.alloc(262144,120))"], { log: path, bytes: 1048576, timeoutMs: 5000 });
        assert.equal(result.code, 0);
        assert.equal((await readFile(path)).length, 262144);
    } finally { await rm(root, { recursive: true }); }
});

test('source aggregate covers the complete sorted regular-file inventory', async () => {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-source-contract-'));
    try {
        for (const directory of SOURCE_ROOTS) { await mkdir(join(root, directory), { recursive: true }); await writeFile(join(root, directory, 'fixture'), 'source'); }
        for (const path of SOURCE_FILES) { await mkdir(join(root, path, '..'), { recursive: true }); await writeFile(join(root, path), path); }
        const source = await qualificationSources(root);
        assert.equal(source.sha256, createHash('sha256').update(JSON.stringify(source.files), 'utf8').digest('hex'));
        assert.deepEqual(source.files.map((file) => file.path), [...source.files.map((file) => file.path)].sort());
        await writeFile(join(root, SOURCE_ROOTS[0], 'fixture'), 'changed');
        assert.notEqual((await qualificationSources(root)).sha256, source.sha256);
    } finally { await rm(root, { recursive: true }); }
});
