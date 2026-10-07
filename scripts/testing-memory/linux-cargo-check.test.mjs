// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { GiB, WORKLOAD_POLICIES } from './policy.mjs';
import { PRESETS } from './presets.mjs';
import { parseOwnedContainers } from './owned-containers.mjs';
import { evaluateLinuxHeadroom, parseLinuxSample, validateLocalEngine } from './linux-engine-headroom.mjs';
import { compilerContainerArgs, compilerSourceManifest, copyCompilerSnapshot, owningCompilerDirectory, validateCompilerImage, validateCompilerRegistry } from './linux-compile-inputs.mjs';
import { createOutputBudget } from './linux-compile-command.mjs';
import { parseCompilerArguments } from './run-linux-cargo-check.mjs';

const uuid = '01234567-89ab-4cde-8f01-23456789abcd';
const imageId = `sha256:${'a'.repeat(64)}`;
const engine = { context: 'desktop-linux', engineId: 'local-engine', kernel: '6.6.87.2-microsoft-standard-WSL2', architecture: 'x86_64' };
const observedAt = Date.parse('2026-10-07T22:00:00Z');
const sample = { ...engine, bootId: uuid, timestamp: new Date(observedAt).toISOString(), totalBytes: 8 * GiB, availableBytes: 5.5 * GiB };

async function temporary(t) {
    const root = await mkdtemp(join(tmpdir(), 'duskcue-linux-helper-'));
    t.after(async () => {
        const canonical = await realpath(root);
        assert.equal(dirname(canonical), await realpath(tmpdir()));
        assert.ok(basename(canonical).startsWith('duskcue-linux-helper-'));
        await rm(canonical, { recursive: true, force: true });
    });
    return root;
}

test('compiler preflight requires its full 4GiB budget plus 1.5GiB Linux reserve', () => {
    assert.equal(evaluateLinuxHeadroom(sample, sample, { nowMs: observedAt }).allowed, true);
    assert.equal(evaluateLinuxHeadroom({ ...sample, availableBytes: sample.availableBytes - 1 }, sample, { nowMs: observedAt }).allowed, false);
    assert.equal(evaluateLinuxHeadroom({ ...sample, availableBytes: 1.5 * GiB }, sample, { nowMs: observedAt, preflight: false }).allowed, true);
    assert.equal(evaluateLinuxHeadroom({ ...sample, availableBytes: 1.5 * GiB - 1 }, sample, { nowMs: observedAt, preflight: false }).allowed, false);
});

test('Linux monitoring fails closed for stale, changed, future or malformed evidence', () => {
    for (const changed of [{ bootId: 'different' }, { engineId: 'remote-engine' }, { context: 'remote-context' }, { availableBytes: NaN }, { availableBytes: 9 * GiB }]) assert.equal(evaluateLinuxHeadroom({ ...sample, ...changed }, sample, { nowMs: observedAt, preflight: false }).allowed, false);
    assert.equal(evaluateLinuxHeadroom(sample, sample, { nowMs: observedAt + 10001 }).allowed, false);
    assert.equal(evaluateLinuxHeadroom(sample, sample, { nowMs: observedAt - 5001 }).allowed, false);
    assert.equal(evaluateLinuxHeadroom(null, sample, { nowMs: observedAt }).allowed, false);
});

test('WSL sample requires actual available memory, boot identity and matching Linux kernel', () => {
    const text = `MemTotal:       8388608 kB\nMemAvailable:   5767168 kB\n${uuid}\nLinux version ${engine.kernel} compiler details\n`;
    assert.equal(parseLinuxSample(text, engine, observedAt).availableBytes, 5.5 * GiB);
    assert.throws(() => parseLinuxSample(text.replace('MemAvailable', 'MemFree'), engine, observedAt));
    assert.throws(() => parseLinuxSample(text, { ...engine, kernel: 'another-kernel' }, observedAt));
});

test('engine qualification rejects remote sockets, non-WSL kernels and non-Desktop engines', () => {
    const info = { id: engine.engineId, kernel: engine.kernel, architecture: engine.architecture, osType: 'linux', name: 'docker-desktop', operatingSystem: 'Docker Desktop' };
    assert.equal(validateLocalEngine(engine.context, 'npipe:////./pipe/dockerDesktopLinuxEngine', info).engineId, engine.engineId);
    assert.throws(() => validateLocalEngine(engine.context, 'tcp://remote:2376', info));
    assert.throws(() => validateLocalEngine(engine.context, 'npipe:////./pipe/dockerDesktopLinuxEngine', { ...info, kernel: 'linuxkit' }));
    assert.throws(() => validateLocalEngine(engine.context, 'npipe:////./pipe/dockerDesktopLinuxEngine', { ...info, name: 'remote' }));
});

test('fixed launch arguments cannot raise compiler budgets or open network/host privilege', () => {
    const args = compilerContainerArgs({ context: engine.context, image: imageId, toolchain: '1.98.0', resourceId: uuid, name: `duskcue-playback-${uuid}`, snapshot: join(tmpdir(), 'source'), artifacts: join(tmpdir(), 'artifacts'), registry: join(tmpdir(), 'registry') });
    for (const expected of ['--pull=never', '--memory=4g', '--memory-swap=4g', '--cpus=2', '--pids-limit=256', '--network=none', '--read-only', '--cap-drop=ALL', '--security-opt=no-new-privileges']) assert.ok(args.includes(expected));
    assert.ok(args.at(-1).endsWith('cargo check -p duskcue --tests --locked --offline -j 2'));
    assert.ok(args.filter((arg) => arg.startsWith('type=bind,')).every((arg) => !arg.includes('docker.sock')));
    assert.equal(args.some((arg) => /privileged|seccomp=unconfined|oom-kill-disable/.test(arg)), false);
    assert.throws(() => compilerContainerArgs({ context: engine.context, image: 'rust:latest' }));
});

test('prepared image must match immutable identity, native architecture and explicit toolchain labels', () => {
    const image = { Id: imageId, Os: 'linux', Architecture: 'amd64', Config: { Labels: { 'duskcue.qualification.compiler': 'linux-cargo-check-v1', 'duskcue.qualification.rust-toolchain': '1.98.0' } } };
    assert.equal(validateCompilerImage(image, imageId, engine.architecture).toolchain, '1.98.0');
    assert.throws(() => validateCompilerImage({ ...image, Architecture: 'arm64' }, imageId, engine.architecture));
    assert.throws(() => validateCompilerImage({ ...image, Id: `sha256:${'b'.repeat(64)}` }, imageId, engine.architecture));
    assert.throws(() => validateCompilerImage({ ...image, Config: { Labels: {} } }, imageId, engine.architecture));
});

test('offline registry and owning workflow must exist before a container can be registered', async (t) => {
    const root = await temporary(t);
    await mkdir(join(root, '.cache/testing-memory'), { recursive: true });
    await assert.rejects(owningCompilerDirectory(root, {}), /owning normal Cargo/);
    await assert.rejects(validateCompilerRegistry(root));
    for (const directory of ['cache', 'index', 'src']) { await mkdir(join(root, directory)); await writeFile(join(root, directory, 'fixture'), 'public offline registry bytes'); }
    assert.equal(await validateCompilerRegistry(root), await realpath(root));
    const marker = join(await realpath(join(root, '.cache/testing-memory')), `${uuid}.containers.jsonl`);
    const environment = { DUSKCUE_TEST_RESOURCE_ID: uuid, DUSKCUE_TEST_RESOURCE_CONTAINERS_FILE: marker };
    await writeFile(join(root, '.cache/testing-memory/active.json'), JSON.stringify({ pid: process.ppid, childPid: process.pid, workload: 'web-unit' }));
    await assert.rejects(owningCompilerDirectory(root, environment), /owning normal Cargo workflow/);
    await writeFile(join(root, '.cache/testing-memory/active.json'), JSON.stringify({ pid: process.ppid, childPid: process.pid + 1, workload: 'linux-cargo-check' }));
    await assert.rejects(owningCompilerDirectory(root, environment), /different child/);
    await writeFile(join(root, '.cache/testing-memory/active.json'), JSON.stringify({ pid: process.ppid, childPid: process.pid, workload: 'linux-cargo-check' }));
    assert.equal((await owningCompilerDirectory(root, environment)).marker, marker);
});

test('owned cleanup retains the exact recorded context and rejects marker context changes', () => {
    const row = { name: `duskcue-playback-${uuid}`, resourceId: uuid, context: 'desktop-linux' };
    assert.equal(parseOwnedContainers(JSON.stringify(row), uuid).get(row.name), row.context);
    assert.equal(parseOwnedContainers(JSON.stringify({ ...row, context: undefined }), uuid).get(row.name), undefined);
    assert.throws(() => parseOwnedContainers(JSON.stringify({ ...row, resourceId: 'different' }), uuid));
    assert.throws(() => parseOwnedContainers(JSON.stringify({ ...row, context: 'tcp://remote' }), uuid));
    assert.throws(() => parseOwnedContainers(`${JSON.stringify(row)}\n${JSON.stringify({ ...row, context: 'other' })}`, uuid));
});

test('source provenance covers locale assets and rejects changed bytes while snapshotting', async (t) => {
    const root = await temporary(t);
    for (const directory of ['server/src', 'server/tests', 'server/assets', 'server/migrations', 'server/locales', 'crates/types/src', 'crates/db/src', 'vendor/plist-1.9.0/src', 'clients/desktop/src-tauri/src']) await mkdir(join(root, directory), { recursive: true });
    for (const path of ['Cargo.toml', 'Cargo.lock', 'server/Cargo.toml', 'server/build.rs', 'server/sqlx.toml', 'crates/types/Cargo.toml', 'crates/db/Cargo.toml', 'vendor/plist-1.9.0/Cargo.toml', 'clients/desktop/src-tauri/Cargo.toml', 'clients/desktop/src-tauri/build.rs', 'server/locales/en.ftl', 'server/src/lib.rs']) await writeFile(join(root, path), path);
    const before = await compilerSourceManifest(root);
    assert.ok(before.sources.some((row) => row.path === 'server/locales/en.ftl'));
    const snapshot = join(root, 'isolated-source');
    await mkdir(snapshot);
    await copyCompilerSnapshot(root, snapshot, before);
    assert.equal((await compilerSourceManifest(snapshot)).sha256, before.sha256);
    await writeFile(join(root, 'server/locales/en.ftl'), 'changed locale bytes');
    assert.notEqual((await compilerSourceManifest(root)).sha256, before.sha256);
    await assert.rejects(copyCompilerSnapshot(root, snapshot, before), /source changed/);
});

test('compiler log cannot exceed its exact bounded byte budget', () => {
    const budget = createOutputBudget(100);
    budget.consume(60);
    budget.consume(40);
    assert.equal(budget.bytes, 100);
    assert.throws(() => budget.consume(1));
    assert.equal(budget.bytes, 100);
});

test('entry-point arguments reject unknown overrides and the preset uses the normal Cargo gate', () => {
    assert.equal(parseCompilerArguments(['--image', imageId, '--registry', 'offline']).image, imageId);
    assert.throws(() => parseCompilerArguments(['--image', 'rust:latest', '--registry', 'offline']));
    assert.throws(() => parseCompilerArguments(['--image', imageId, '--registry', 'offline', '--privileged']));
    assert.equal(PRESETS['linux-cargo-check'].policy, 'cargo');
    assert.equal(PRESETS['linux-cargo-check'].heapMiB, 128);
    assert.equal(WORKLOAD_POLICIES.cargo.minCommitHeadroomBytes, 12 * GiB);
});
