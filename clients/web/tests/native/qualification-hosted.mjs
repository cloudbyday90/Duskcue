// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import { createHash } from 'node:crypto';
import { createReadStream } from 'node:fs';
import { mkdir, readFile, realpath, writeFile } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHostedMetadataReader, retainHostedProbe } from './qualification-metadata.mjs';

const workspace = fileURLToPath(new URL('../../../../', import.meta.url));
const evidencePath = join(workspace, '.cache', 'tonight-desktop', 'hosted-ci-prerequisites.json');
const metadata = createHostedMetadataReader({ cwd: workspace });

export function hostedContext(environment, root, platform = process.platform) {
    if (platform !== 'win32' || environment.GITHUB_ACTIONS !== 'true' || environment.RUNNER_ENVIRONMENT !== 'github-hosted' || environment.RUNNER_OS !== 'Windows'
        || !environment.GITHUB_WORKSPACE || resolve(environment.GITHUB_WORKSPACE).toLowerCase() !== resolve(root).toLowerCase()
        || !/^[a-f0-9]{40}$/.test(environment.GITHUB_SHA || '') || !/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(environment.GITHUB_REPOSITORY || '')
        || !/^\d+$/.test(environment.GITHUB_RUN_ID || '') || !/^\d+$/.test(environment.GITHUB_RUN_ATTEMPT || '') || !/^[A-Za-z0-9_-]+$/.test(environment.GITHUB_JOB || '')) throw new Error('Native hosted preparation requires the matching GitHub-hosted Windows checkout and run identity.');
    return { repository: environment.GITHUB_REPOSITORY, sourceCommit: environment.GITHUB_SHA, runId: environment.GITHUB_RUN_ID, attempt: environment.GITHUB_RUN_ATTEMPT, job: environment.GITHUB_JOB, runnerOS: environment.RUNNER_OS, runnerEnvironment: environment.RUNNER_ENVIRONMENT, imageOS: environment.ImageOS || null, imageVersion: environment.ImageVersion || null };
}

export function validateHostedPrerequisites(system, encoderOutput) {
    if (!Array.isArray(system?.webviewVersions) || !system.webviewVersions.some((version) => /^\d+\.\d+\.\d+\.\d+$/.test(version) && version !== '0.0.0.0')) throw new Error('Install and verify the actual Evergreen WebView2 Runtime before native qualification.');
    if (system.userInteractive !== true || !Number.isSafeInteger(system.sessionId) || system.sessionId <= 0) throw new Error('Hosted native qualification requires an interactive Windows graphical session.');
    const encoders = new Set([...encoderOutput.matchAll(/^\s*[VAS][A-Z.]{5}\s+(\S+)/gm)].map((match) => match[1]));
    for (const name of ['libx264', 'aac', 'libwebp']) if (!encoders.has(name)) throw new Error(`Hosted FFmpeg is missing the ${name} fixture encoder.`);
    return { encoders: ['libx264', 'aac', 'libwebp'], webviewVersions: system.webviewVersions, userInteractive: system.userInteractive, sessionId: system.sessionId, display: system.display || null };
}

export function validateHostedProof(proof, context, nowMs = Date.now()) {
    const ageMs = nowMs - Date.parse(proof?.checkedAt);
    if (proof?.version !== 1 || proof.kind !== 'native-hosted-prerequisites' || proof.status !== 'passed' || !Number.isFinite(ageMs) || ageMs < -5000 || ageMs > 60 * 60 * 1000
        || ['repository', 'sourceCommit', 'runId', 'attempt', 'job', 'runnerOS', 'runnerEnvironment'].some((key) => proof.context?.[key] !== context[key])) throw new Error('Run the hosted prerequisite probe for this exact checkout/job before preparing or running native qualification.');
    return proof;
}

async function currentCommit() {
    const commit = await metadata('git', ['rev-parse', 'HEAD'], 'checkout_commit');
    if (!/^[a-f0-9]{40}$/.test(commit)) throw new Error('The actual native checkout commit is unavailable.');
    return commit;
}

async function staticIndexHash() {
    const hash = createHash('sha256');
    for await (const chunk of createReadStream(join(workspace, 'clients', 'web', 'build', 'client', 'index.html'), { highWaterMark: 65536 })) hash.update(chunk);
    return hash.digest('hex');
}

export async function hostedPreparationProvenance() {
    if (process.env.GITHUB_ACTIONS !== 'true') return undefined;
    const context = hostedContext(process.env, workspace);
    if (await currentCommit() !== context.sourceCommit) throw new Error('The native checkout differs from GITHUB_SHA.');
    const proof = validateHostedProof(JSON.parse(await readFile(evidencePath, 'utf8')), context);
    if (!process.env.DUSKCUE_TEST_FFMPEG || await realpath(process.env.DUSKCUE_TEST_FFMPEG) !== proof.ffmpeg.path) throw new Error('Use the exact explicitly probed host FFmpeg binary; Docker fallback is unavailable for hosted native qualification.');
    return { context, checkedAt: proof.checkedAt, prerequisites: proof.prerequisites, ffmpeg: proof.ffmpeg, node: process.version, staticIndexSha256: await staticIndexHash() };
}

export async function validateHostedManifest(manifest) {
    if (process.env.GITHUB_ACTIONS !== 'true' && !manifest.hostedCI) return;
    const current = await hostedPreparationProvenance();
    if (!current || !manifest.hostedCI || JSON.stringify(current.context) !== JSON.stringify(manifest.hostedCI.context)
        || current.staticIndexSha256 !== manifest.hostedCI.staticIndexSha256 || current.ffmpeg.path !== manifest.hostedCI.ffmpeg?.path) throw new Error('Hosted native build provenance changed after fresh preparation.');
}

async function probe() {
    const context = hostedContext(process.env, workspace);
    const { result, failure } = await retainHostedProbe(context, async () => {
        if (await currentCommit() !== context.sourceCommit) throw new Error('The actual checkout differs from GITHUB_SHA.');
        if (!isAbsolute(process.env.DUSKCUE_TEST_FFMPEG || '')) throw new Error('Set DUSKCUE_TEST_FFMPEG to the installed host encoder path before probing.');
        const path = await realpath(process.env.DUSKCUE_TEST_FFMPEG);
        const system = JSON.parse(await metadata('pwsh.exe', ['-NoProfile', '-File', fileURLToPath(new URL('./qualification-hosted.ps1', import.meta.url))], 'system_graphics'));
        const version = (await metadata(path, ['-version'], 'ffmpeg_version')).split(/\r?\n/)[0];
        const prerequisites = validateHostedPrerequisites(system, await metadata(path, ['-hide_banner', '-encoders'], 'ffmpeg_encoders'));
        return { prerequisites, ffmpeg: { path, version } };
    }, async (proof) => {
        await mkdir(join(workspace, '.cache', 'tonight-desktop'), { recursive: true });
        await writeFile(evidencePath, `${JSON.stringify(proof, null, 2)}\n`, 'utf8');
    });
    console.log(JSON.stringify({ status: result.status, result: evidencePath }));
    if (failure) throw failure;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    try {
        if (process.argv[2] !== '--probe' || process.argv.length !== 3) throw new Error('Usage: node qualification-hosted.mjs --probe');
        await probe();
    } catch (error) { console.error(error.message); process.exitCode = 1; }
}
