// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { copyFile, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, relative } from 'node:path';
import { qualificationSources, hashFile } from './source.mjs';
import { runBounded } from './commands.mjs';
import { assertNativeArchitecture, validateRustHost, verifyElfArchitecture } from './architecture.mjs';

const source = '/src';
const output = '/out';

function artifactSelector(line, name) {
    let entry;
    try { entry = JSON.parse(line); } catch { return null; }
    if (entry.reason !== 'compiler-artifact' || entry.target?.name !== name || entry.profile?.test !== true || !entry.executable) return null;
    const path = relative('/src/target/debug/deps', entry.executable);
    if (!path || path.startsWith('..') || path.includes('/') || path.includes('\\')) throw new Error('Cargo test artifact escaped its compiled deps directory.');
    return entry.executable;
}

async function checked(program, args, options) {
    const result = await runBounded(program, args, { ...options, forward: true });
    if (result.code !== 0 || result.signal) throw new Error('Qualification artifact compilation failed; inspect the bounded compiler log.');
    return result;
}

async function main() {
    if (process.platform !== 'linux' || !/^[a-f0-9]{40}$/.test(process.env.DUSKCUE_SOURCE_COMMIT || '')) throw new Error('Qualification compilation requires Linux and the exact source commit.');
    const { architecture } = assertNativeArchitecture(process.env.DUSKCUE_QUALIFICATION_ARCH, process.arch);
    await mkdir(output, { recursive: true });
    const inputs = await qualificationSources(source);
    const expected = JSON.parse(await readFile('/src/qualification-inputs.json', 'utf8'));
    if (inputs.sha256 !== expected.sha256 || JSON.stringify(inputs.files) !== JSON.stringify(expected.files)) throw new Error('Compiler source inventory does not match the approved checkout.');
    const result = { version: 1, kind: 'duskcue-linux-test-artifacts', status: 'in_progress', architecture, source: { commit: process.env.DUSKCUE_SOURCE_COMMIT, ...inputs }, debugInfo: 0, cargoJobs: 2, artifacts: {} };
    await writeFile(join(output, 'artifact-manifest.json'), `${JSON.stringify(result, null, 2)}\n`);
    await checked('cc', ['-std=c11', '-O2', '-fPIC', '-fstack-protector-strong', '-Wall', '-Wextra', '-Werror', '-shared', '-Wl,-z,now,-z,relro,-soname,/usr/local/lib/duskcue-ffmpeg-bootstrap.so', '-o', '/out/duskcue-ffmpeg-bootstrap.so', '/src/native/ffmpeg-bootstrap/bootstrap.c'], { cwd: source, log: join(output, 'bootstrap-compile.log'), timeoutMs: 60000 });
    await checked('cc', ['-std=c11', '-O2', '-pthread', '-Wall', '-Wextra', '-Werror', '-Wl,-z,now,--no-as-needed', '-o', '/out/duskcue-ffmpeg-probe', '/src/native/ffmpeg-bootstrap/probe.c', '/out/duskcue-ffmpeg-bootstrap.so'], { cwd: source, log: join(output, 'probe-compile.log'), timeoutMs: 60000 });
    const environment = { ...process.env, CARGO_BUILD_JOBS: '2', CARGO_PROFILE_DEV_DEBUG: '0', CARGO_PROFILE_TEST_DEBUG: '0' };
    for (const [key, target, args] of [['libTest', 'duskcue', ['--lib']], ['stopContract', 'playback_stop_contract', ['--test', 'playback_stop_contract']]]) {
        let executable;
        await checked('cargo', ['test', '-p', 'duskcue', '--locked', '-j', '2', ...args, '--no-run', '--message-format=json'], { cwd: source, env: environment, log: join(output, `${key}-compile.log`), timeoutMs: 1800000, bytes: 33554432, onLine: (line) => {
            const selected = artifactSelector(line, target);
            if (selected) { if (executable && executable !== selected) throw new Error('Cargo produced ambiguous test executables.'); executable = selected; }
        } });
        if (!executable) throw new Error('Cargo did not emit the requested current test executable.');
        const name = key === 'libTest' ? 'duskcue-lib-tests' : 'playback-stop-contract';
        await copyFile(executable, join(output, name));
        result.artifacts[key] = { path: `build/${name}`, sha256: await hashFile(join(output, name)) };
    }
    for (const [key, name] of [['bootstrap', 'duskcue-ffmpeg-bootstrap.so'], ['probe', 'duskcue-ffmpeg-probe']]) result.artifacts[key] = { path: `build/${name}`, sha256: await hashFile(join(output, name)) };
    for (const artifact of Object.values(result.artifacts)) await verifyElfArchitecture(join(output, artifact.path.slice('build/'.length)), architecture);
    try {
        const lint = await runBounded('cargo', ['clippy', '-p', 'duskcue', '--all-targets', '--locked', '-j', '2', '--', '-D', 'warnings'], { cwd: source, env: environment, log: join(output, 'strict-clippy.log'), timeoutMs: 1800000, bytes: 33554432, forward: true });
        result.lint = { passed: lint.code === 0 && !lint.signal, exitCode: lint.code, signal: lint.signal, log: 'build/strict-clippy.log' };
    } catch (error) { result.lint = { passed: false, log: 'build/strict-clippy.log', error: error.message }; }
    const rust = await checked('rustc', ['-Vv'], { timeoutMs: 30000 });
    validateRustHost(rust.stdout, architecture);
    const cargo = await checked('cargo', ['-V'], { timeoutMs: 30000 });
    result.toolchain = { rust: rust.stdout.trim(), cargo: cargo.stdout.trim() };
    if ((await qualificationSources(source)).sha256 !== inputs.sha256) throw new Error('Sources changed during qualification compilation.');
    result.status = 'passed';
    await writeFile(join(output, 'artifact-manifest.json'), `${JSON.stringify(result, null, 2)}\n`);
}

main().catch((error) => { console.error(error.message); process.exitCode = 1; });
