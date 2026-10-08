// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { randomBytes, randomUUID } from 'node:crypto';
import { appendFile, copyFile, lstat, mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { qualificationSources, hashFile } from './source.mjs';
import { runBounded } from './commands.mjs';
import { docker, metadata, ownedDocker } from './owned.mjs';
import { REQUIRED_MANAGED_CASES, PRODUCER_CASE, selectedUnitCases } from './cases.mjs';
import { STOP_CASE } from './runtime.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));
export const PRODUCER_FILES = Object.freeze(['source.mkv', 'first.srt', 'second.srt', 'caption/manifest.m3u8', 'caption/init.mp4', ...Array.from({ length: 4 }, (_, index) => `caption/seg_${String(index).padStart(4, '0')}.m4s`)]);

export function within(root, path) {
    const value = relative(root, path);
    return value !== '' && value !== '..' && !value.startsWith(`..${sep}`) && !isAbsolute(value);
}

export function parseArguments(args) {
    if (args.length !== 4 || args[0] !== '--commit' || !/^[a-f0-9]{40}$/.test(args[1]) || args[2] !== '--output' || !args[3]) throw new Error('Provide exact --commit <Git SHA> and --output <owned RUNNER_TEMP directory>.');
    return { commit: args[1], output: args[3] };
}

function marker(output, prefix) {
    const lines = output.split(/\r?\n/).filter((line) => line.startsWith(prefix));
    if (lines.length !== 1) throw new Error(`Expected exactly one ${prefix} result.`);
    return JSON.parse(lines[0].slice(prefix.length));
}

async function checked(args, options = {}) {
    const result = await docker(args, options);
    if (result.code !== 0 || result.signal) throw new Error('Qualification image preparation failed; inspect its bounded log.');
    return result;
}

async function image(tag, commit, sha256) {
    const info = await metadata(['image', 'inspect', tag, '--format', '{{json .}}']);
    if (!/^sha256:[a-f0-9]{64}$/.test(info.Id) || info.Architecture !== 'amd64' || info.Os !== 'linux'
        || info.Config?.Labels?.['duskcue.qualification.commit'] !== commit || info.Config?.Labels?.['duskcue.qualification.source-sha256'] !== sha256) throw new Error('Qualification image provenance or architecture does not match.');
    return { id: info.Id, architecture: info.Architecture };
}

async function copySnapshot(root, destination, source) {
    await mkdir(destination, { recursive: true });
    for (const file of source.files) {
        const target = join(destination, file.path);
        await mkdir(dirname(target), { recursive: true });
        await copyFile(join(root, file.path), target);
    }
    await writeFile(join(destination, 'qualification-inputs.json'), `${JSON.stringify(source)}\n`);
    await writeFile(join(destination, '.dockerignore'), '.git\nnode_modules\n**/node_modules\ntarget\n**/target\n.env\n.env.*\n');
    const copied = await qualificationSources(destination);
    if (copied.sha256 !== source.sha256 || JSON.stringify(copied.files) !== JSON.stringify(source.files)) throw new Error('Qualification snapshot changed during copy.');
}

async function runtimeContext(snapshot, build, destination) {
    await mkdir(join(destination, 'build'), { recursive: true });
    for (const file of ['artifact-manifest.json', 'duskcue-lib-tests', 'playback-stop-contract', 'duskcue-ffmpeg-bootstrap.so', 'duskcue-ffmpeg-probe']) await copyFile(join(build, file), join(destination, 'build', file));
    for (const path of ['docker/fonts', 'scripts/qualification']) {
        const source = await qualificationSources(snapshot);
        for (const file of source.files.filter((file) => file.path.startsWith(`${path}/`))) {
            const target = join(destination, file.path);
            await mkdir(dirname(target), { recursive: true });
            await copyFile(join(snapshot, file.path), target);
        }
    }
}

async function host(options) {
    if (process.platform !== 'linux' || process.arch !== 'x64' || process.env.GITHUB_ACTIONS !== 'true'
        || process.env.RUNNER_ENVIRONMENT !== 'github-hosted' || process.env.RUNNER_OS !== 'Linux' || process.env.GITHUB_SHA !== options.commit) throw new Error('This qualification is restricted to the matching GitHub-hosted Linux checkout.');
    const current = await realpath(workspace);
    if (current !== await realpath(process.env.GITHUB_WORKSPACE || '') || current !== await realpath(process.cwd())) throw new Error('Qualification workspace does not match the hosted checkout.');
    const head = await runBounded('git', ['rev-parse', 'HEAD'], { cwd: workspace, timeoutMs: 10000 });
    const dirty = await runBounded('git', ['status', '--porcelain'], { cwd: workspace, timeoutMs: 10000 });
    if (head.code !== 0 || head.stdout.trim() !== options.commit || dirty.code !== 0 || dirty.stdout.trim()) throw new Error('Qualification requires the exact clean committed checkout.');
    if (process.env.DOCKER_HOST && process.env.DOCKER_HOST !== 'unix:///var/run/docker.sock') throw new Error('Qualification requires the hosted local Docker engine.');
    const temp = await realpath(process.env.RUNNER_TEMP || '');
    const output = resolve(options.output);
    if (!within(temp, output)) throw new Error('Qualification output escaped RUNNER_TEMP.');
    await mkdir(output, { recursive: true });
    if (!within(temp, await realpath(output)) || (await lstat(output)).isSymbolicLink()) throw new Error('Qualification output is not an owned regular directory.');
    return { temp, output };
}

async function main() {
    const options = parseArguments(process.argv.slice(2));
    const location = await host(options);
    const resourceId = randomUUID();
    const directory = join(location.output, resourceId);
    const work = join(location.temp, `tonight-linux-work-${resourceId}`);
    await mkdir(directory);
    await mkdir(work);
    await mkdir(join(directory, 'logs'));
    const ownership = ownedDocker(directory, resourceId);
    const result = { version: 1, kind: 'duskcue-linux-qualification', status: 'in_progress', resourceId, startedAt: new Date().toISOString(), source: { commit: options.commit }, artifacts: {}, cases: [], cleanup: { passed: false }, limits: { compiler: { memoryBytes: 4294967296, memorySwapBytes: 4294967296, cpus: 2, pids: 256, cargoJobs: 2, debugInfo: 0 }, runtime: { memoryBytes: 536870912, memorySwapBytes: 536870912, cpus: 2, pids: 128 } } };
    const save = () => writeFile(join(directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`);
    if (process.env.GITHUB_OUTPUT) await appendFile(process.env.GITHUB_OUTPUT, `artifact_dir=${directory}\n`);
    await save();
    let failed = false;
    try {
        const source = await qualificationSources(workspace);
        result.source = { commit: options.commit, ...source };
        const snapshot = join(work, 'source');
        await copySnapshot(workspace, snapshot, source);
        const recipe = join(snapshot, 'docker/qualification/Dockerfile.linux');
        const compilerTag = `duskcue-qualification-compiler:${resourceId}`;
        await checked(['build', '--platform=linux/amd64', '--target=compiler', '-f', recipe, '--build-arg', `SOURCE_COMMIT=${options.commit}`, '--build-arg', `SOURCE_SHA256=${source.sha256}`, '-t', compilerTag, snapshot], { log: join(directory, 'logs/compiler-image.log'), timeoutMs: 600000, bytes: 33554432, forward: true });
        result.compilerImage = await image(compilerTag, options.commit, source.sha256);
        const compiler = await ownership.create(result.compilerImage.id, [], { memory: 4294967296, pids: 256, network: 'bridge', environment: { DUSKCUE_SOURCE_COMMIT: options.commit, CARGO_BUILD_JOBS: '2', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_PROFILE_TEST_DEBUG: '0' } });
        const compiled = await ownership.start(compiler, { log: join(directory, 'logs/compiler.log'), timeoutMs: 4500000, bytes: 67108864, forward: true });
        result.compilation = await ownership.inspect(compiler);
        if (compiled.code !== 0 || compiled.signal || result.compilation.state.exitCode !== 0 || result.compilation.state.oomKilled) throw new Error('Current bounded Linux artifact compilation failed.');
        const build = join(directory, 'build');
        await mkdir(build);
        await ownership.copyOut(compiler, '/out/.', build);
        await ownership.remove(compiler);
        const compiledManifest = JSON.parse(await readFile(join(build, 'artifact-manifest.json'), 'utf8'));
        if (compiledManifest.status !== 'passed' || compiledManifest.source.commit !== options.commit || compiledManifest.source.sha256 !== source.sha256 || JSON.stringify(compiledManifest.source.files) !== JSON.stringify(source.files)) throw new Error('Exported current artifact provenance mismatched.');
        result.artifacts = compiledManifest.artifacts;
        result.lint = compiledManifest.lint;
        for (const artifact of Object.values(result.artifacts)) if (!within(directory, resolve(directory, artifact.path)) || await hashFile(join(directory, artifact.path)) !== artifact.sha256) throw new Error('Exported compiler artifact hash mismatched.');
        const context = join(work, 'runtime');
        await runtimeContext(snapshot, build, context);
        const runtimeTag = `duskcue-qualification-runtime:${resourceId}`;
        await checked(['build', '--platform=linux/amd64', '--target=runtime', '-f', recipe, '--build-arg', `SOURCE_COMMIT=${options.commit}`, '--build-arg', `SOURCE_SHA256=${source.sha256}`, '-t', runtimeTag, context], { log: join(directory, 'logs/runtime-image.log'), timeoutMs: 600000, bytes: 33554432, forward: true });
        result.image = await image(runtimeTag, options.commit, source.sha256);
        const environment = { DUSKCUE_SOURCE_COMMIT: options.commit, DUSKCUE_TEST_RESOURCE_ID: resourceId, RUST_TEST_THREADS: '1', DUSKCUE_TEST_FFMPEG_MODE: 'linux-local-managed', DUSKCUE_TEST_PLAYBACK_SOURCE: '/fixtures/source.mkv' };
        const inventoryContainer = await ownership.create(result.image.id, ['inventory'], { environment });
        const inventoryRun = await ownership.start(inventoryContainer, { log: join(directory, 'logs/inventory.log') });
        if (inventoryRun.code !== 0 || inventoryRun.signal) throw new Error('Current runtime test inventory failed.');
        const inventory = marker(inventoryRun.stdout, 'DUSKCUE_TEST_INVENTORY=');
        await ownership.copyOut(inventoryContainer, '/qualification/runtime-provenance.json', join(directory, 'runtime-provenance.json'));
        await ownership.remove(inventoryContainer);
        const tests = [PRODUCER_CASE, ...REQUIRED_MANAGED_CASES, ...selectedUnitCases(inventory)];
        let sequence = 0;
        for (const test of tests) {
            const log = `logs/case-${String(sequence++).padStart(3, '0')}.log`;
            const container = await ownership.create(result.image.id, ['case', test], { environment });
            const sourceClip = result.diagnosticSource?.path ? join(directory, result.diagnosticSource.path) : join(directory, 'producer/source.mkv');
            if (test !== PRODUCER_CASE && REQUIRED_MANAGED_CASES.slice(0, 2).includes(test)) {
                try { await ownership.copyIn(container, sourceClip, '/fixtures/source.mkv'); }
                catch { result.cases.push({ test, passed: false, exactTests: 0, imageId: result.image.id, log, error: 'Fresh producer source is unavailable.' }); failed = true; await ownership.remove(container); continue; }
            }
            try {
                const execution = await ownership.start(container, { log: join(directory, log) });
                const outcome = marker(execution.stdout, 'DUSKCUE_RUNTIME_RESULT=');
                const record = { test, passed: outcome.test === test && outcome.passed === true && outcome.exactTests === 1 && execution.code === 0 && !execution.signal && outcome.sourceCommit === options.commit && outcome.sourceSha256 === source.sha256, exactTests: outcome.exactTests, exitCode: execution.code, log, imageId: result.image.id };
                result.cases.push(record);
                failed ||= !record.passed;
                if (test === PRODUCER_CASE) {
                    result.producer = { ...record, directory: 'producer', captionDirectory: 'producer/caption', files: [] };
                    try {
                        const retainedSource = join(directory, 'diagnostics/source.mkv');
                        await mkdir(dirname(retainedSource), { recursive: true });
                        await ownership.copyOut(container, '/fixtures/source.mkv', retainedSource, { requireSuccess: false });
                        const file = await lstat(retainedSource);
                        if (!file.isFile() || file.isSymbolicLink() || file.size > 16777216) throw new Error('Diagnostic source is not a bounded regular fixture.');
                        result.diagnosticSource = { path: 'diagnostics/source.mkv', bytes: file.size, sha256: await hashFile(retainedSource), producerPassed: record.passed };
                    } catch { result.diagnosticSource = null; }
                    if (record.passed) {
                        const produced = marker(execution.stdout, 'DUSKCUE_FFMPEG_FIXTURE=');
                        if (produced.directory !== '/fixtures' || produced.audio_default_index !== 2 || produced.subtitle_ordinal !== 1) throw new Error('Producer marker did not describe the exact current fixture.');
                        result.producer.metrics = produced;
                        for (const file of PRODUCER_FILES) {
                            const destination = join(directory, 'producer', file);
                            await mkdir(dirname(destination), { recursive: true });
                            await ownership.copyOut(container, `/fixtures/${file}`, destination);
                            const meta = await lstat(destination);
                            if (!meta.isFile() || meta.isSymbolicLink()) throw new Error('Producer exported a nonregular asset.');
                            result.producer.files.push({ path: `producer/${file}`, bytes: meta.size, sha256: await hashFile(destination) });
                        }
                    }
                }
            } catch (error) {
                failed = true;
                const current = result.cases.find((record) => record.test === test);
                if (current) { current.passed = false; current.error = error.message; }
                else result.cases.push({ test, passed: false, exactTests: 0, log, imageId: result.image.id, error: error.message });
                if (test === PRODUCER_CASE && result.producer) { result.producer.passed = false; result.producer.error = error.message; }
            }
            finally { await ownership.remove(container); await save(); }
        }
        await checked(['pull', 'postgres:18'], { log: join(directory, 'logs/postgres-image.log'), timeoutMs: 300000 });
        const postgres = await metadata(['image', 'inspect', 'postgres:18', '--format', '{{json .}}']);
        if (!/^sha256:[a-f0-9]{64}$/.test(postgres.Id)) throw new Error('Disposable PostgreSQL image identity unavailable.');
        result.postgresImage = { id: postgres.Id };
        const password = randomBytes(24).toString('base64url');
        const database = `duskcue_transcode_test_${resourceId.replaceAll('-', '')}`;
        const databaseUrl = `postgres://duskcue_fixture:${password}@127.0.0.1:5432/${database}`;
        const pg = await ownership.create(postgres.Id, [], { network: 'bridge', user: 'postgres', environment: { POSTGRES_DB: database, POSTGRES_USER: 'duskcue_fixture', POSTGRES_PASSWORD: password, PGDATA: '/tmp/duskcue-postgres' } });
        await ownership.startDetached(pg);
        let ready = false;
        for (let attempt = 0; attempt < 150; attempt += 1) {
            const probe = await docker(['exec', pg, 'pg_isready', '-U', 'duskcue_fixture', '-d', database]);
            if (probe.code === 0) { ready = true; break; }
            await new Promise((resolveReady) => setTimeout(resolveReady, 200));
        }
        if (!ready) throw new Error('Owned disposable PostgreSQL did not become ready.');
        const sql = await ownership.create(result.image.id, ['sql', STOP_CASE], { network: `container:${pg}`, environment: { ...environment, DUSKCUE_DATABASE_URL: databaseUrl, DUSKCUE_TRANSCODE_ACCESS_TESTS: 'disposable', DUSKCUE_TEST_PLAYBACK_SOURCE: '/src/.cache/tonight-server-ffmpeg/source.mkv' } });
        try {
            const sqlSource = result.diagnosticSource?.path ? join(directory, result.diagnosticSource.path) : join(directory, 'producer/source.mkv');
            await ownership.copyIn(sql, sqlSource, '/src/.cache/tonight-server-ffmpeg/source.mkv');
            const execution = await ownership.start(sql, { log: join(directory, 'logs/stop-contract.log'), redact: [databaseUrl, password] });
            const outcome = marker(execution.stdout, 'DUSKCUE_RUNTIME_RESULT=');
            result.sql = { test: STOP_CASE, passed: outcome.passed === true && outcome.exactTests === 1 && execution.code === 0 && !execution.signal, exactTests: outcome.exactTests, exitCode: execution.code, log: 'logs/stop-contract.log', imageId: result.image.id };
            failed ||= !result.sql.passed;
        } finally { await ownership.remove(sql); await ownership.remove(pg); }
        if ((await qualificationSources(workspace)).sha256 !== source.sha256) throw new Error('Checkout sources changed during qualification.');
        result.status = failed ? 'failed' : 'passed';
    } catch (error) { result.status = 'failed'; result.error = error.message; failed = true; }
    finally {
        try { result.cleanup = { passed: true, removed: await ownership.cleanup() }; }
        catch (error) { result.status = 'failed'; result.cleanup = { passed: false, error: error.message }; failed = true; }
        result.finishedAt = new Date().toISOString();
        await save();
    }
    console.log(JSON.stringify({ status: result.status, artifactDirectory: directory, strictClippy: result.lint?.passed === true ? 'passed' : 'not-passed' }));
    if (failed || result.status !== 'passed') process.exitCode = 1;
}

if (process.argv[1]?.endsWith('/linux.mjs')) main().catch((error) => { console.error(error.message); process.exitCode = 1; });
