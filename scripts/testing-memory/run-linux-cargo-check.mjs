// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { appendFile, mkdir, open, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cleanupOwnedContainers } from './owned-containers.mjs';
import { boundedMetadata, evaluateLinuxHeadroom, readLinuxSample, readLocalEngine } from './linux-engine-headroom.mjs';
import { compilerContainerArgs, compilerSourceManifest, copyCompilerSnapshot, owningCompilerDirectory, validateCompilerImage, validateCompilerRegistry } from './linux-compile-inputs.mjs';
import { runCompilerCommand } from './linux-compile-command.mjs';

const workspace = fileURLToPath(new URL('../../', import.meta.url));

export function parseCompilerArguments(args) {
    if (args.length !== 4 || args[0] !== '--image' || !/^sha256:[a-f0-9]{64}$/.test(args[1]) || args[2] !== '--registry' || !args[3]) throw new Error('Provide --image <immutable local prepared SHA256> --registry <prepared offline Cargo registry directory>.');
    return { image: args[1], registry: args[3] };
}

async function main() {
    const options = parseCompilerArguments(process.argv.slice(2));
    if (process.platform !== 'win32') throw new Error('Use the owning Windows memory workflow for this local Docker Desktop qualification.');
    const owner = await owningCompilerDirectory(workspace, process.env);
    const result = { version: 1, kind: 'linux-cargo-check', startedAt: new Date().toISOString(), resourceId: owner.resourceId, status: 'in_progress', limits: { memoryGiB: 4, swapGiB: 0, cpus: 2, pids: 256, cargoJobs: 2, nodeHeapMiB: 128, deadlineMinutes: 30, logMiB: 32 }, limitations: ['This checks Linux backend and test compilation; it does not execute tests, qualify the FFmpeg sandbox, build a production image or deploy the application.'] };
    let failure;
    try {
        const registry = await validateCompilerRegistry(options.registry);
        const engine = await readLocalEngine();
        const image = JSON.parse(await boundedMetadata('docker.exe', ['--context', engine.context, 'image', 'inspect', options.image, '--format', '{{json .}}']));
        result.image = validateCompilerImage(image, options.image, engine.architecture);
        result.engine = engine;
        const initial = await readLinuxSample(engine);
        const before = evaluateLinuxHeadroom(initial, initial);
        if (!before.allowed) throw new Error(before.reason);
        const snapshot = join(owner.directory, 'source');
        const artifacts = join(owner.directory, 'artifacts');
        await mkdir(snapshot);
        for (const directory of ['cargo', 'target', 'home']) await mkdir(join(artifacts, directory), { recursive: true });
        result.source = await compilerSourceManifest(workspace);
        await copyCompilerSnapshot(workspace, snapshot, result.source);
        await writeFile(join(owner.directory, 'source-manifest.json'), `${JSON.stringify(result.source, null, 2)}\n`);
        result.registry = registry;
        const current = await readLinuxSample(engine);
        const preflight = evaluateLinuxHeadroom(current, initial);
        if (!preflight.allowed) throw new Error(preflight.reason);
        result.linuxPreflight = { sample: current, decision: preflight };
        const samplePath = join(owner.directory, 'linux-memory.jsonl');
        const recordSample = (sample) => appendFile(samplePath, `${JSON.stringify(sample)}\n`, 'utf8');
        await recordSample(current);
        const name = `duskcue-playback-${owner.resourceId}`;
        const args = compilerContainerArgs({ context: engine.context, image: result.image.id, toolchain: result.image.toolchain, resourceId: owner.resourceId, name, snapshot, artifacts, registry });
        await appendFile(owner.marker, `${JSON.stringify({ resourceId: owner.resourceId, name, context: engine.context })}\n`, 'utf8');
        result.container = { name, context: engine.context };
        result.compilation = await runCompilerCommand(workspace, args, owner, engine, current, recordSample);
        const file = await open(join(owner.directory, 'compiler.log'));
        try {
            const buffer = Buffer.alloc(16384);
            const { bytesRead } = await file.read(buffer, 0, buffer.length, 0);
            const text = buffer.toString('utf8', 0, bytesRead);
            const host = /^host: (\S+)$/m.exec(text)?.[1];
            if (!host?.includes('-linux-')) throw new Error('The compiler did not report an actual Linux Rust host.');
            result.toolchain = { host, rustc: /^rustc (.+)$/m.exec(text)?.[1], cargo: /^cargo (.+)$/m.exec(text)?.[1] };
        } finally { await file.close(); }
        if ((await compilerSourceManifest(workspace)).sha256 !== result.source.sha256 || (await compilerSourceManifest(snapshot)).sha256 !== result.source.sha256) throw new Error('Sources changed during Linux compilation; this result cannot qualify the current tree.');
        result.status = 'passed';
    } catch (error) { result.status = 'failed'; result.error = error.message; failure = error; }
    finally {
        try { result.removedOwnedContainers = await cleanupOwnedContainers(owner.marker, owner.resourceId); }
        catch (error) { result.status = 'failed'; result.cleanupError = error.message; failure ||= error; }
        result.finishedAt = new Date().toISOString();
        await writeFile(join(owner.directory, 'result.json'), `${JSON.stringify(result, null, 2)}\n`, 'utf8');
    }
    console.log(JSON.stringify({ status: result.status, result: join(owner.directory, 'result.json') }));
    if (failure) throw failure;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try { await main(); }
    catch (error) { console.error(error.message); process.exitCode = 1; }
}
