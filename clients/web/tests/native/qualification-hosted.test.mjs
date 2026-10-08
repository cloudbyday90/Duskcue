// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rmdir, unlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { hostedContext, validateHostedPrerequisites, validateHostedProof } from './qualification-hosted.mjs';
import { createHostedMetadataReader, hostedMetadataFailure, retainHostedProbe } from './qualification-metadata.mjs';

const root = 'C:/hosted/Duskcue';
const environment = { GITHUB_ACTIONS: 'true', RUNNER_ENVIRONMENT: 'github-hosted', RUNNER_OS: 'Windows', GITHUB_WORKSPACE: root, GITHUB_SHA: 'a'.repeat(40), GITHUB_REPOSITORY: 'cloudbyday90/Duskcue', GITHUB_RUN_ID: '123', GITHUB_RUN_ATTEMPT: '1', GITHUB_JOB: 'native_windows' };
const encoders = ' V....D libx264 H.264\n A..... aac AAC\n V....D libwebp WebP\n';

test('hosted probe rejects local/self-hosted/other-platform or mismatched checkout context', () => {
    assert.equal(hostedContext(environment, root, 'win32').sourceCommit, environment.GITHUB_SHA);
    for (const changed of [{ GITHUB_ACTIONS: undefined }, { RUNNER_ENVIRONMENT: 'self-hosted' }, { RUNNER_OS: 'Linux' }, { GITHUB_WORKSPACE: 'C:/another/repo' }, { GITHUB_SHA: 'invalid' }]) assert.throws(() => hostedContext({ ...environment, ...changed }, root, 'win32'));
    assert.throws(() => hostedContext(environment, root, 'linux'));
});

test('real Runtime registration and graphical session are prerequisites rather than an Edge browser assumption', () => {
    const system = { webviewVersions: ['152.0.4191.66'], userInteractive: true, sessionId: 1 };
    assert.deepEqual(validateHostedPrerequisites(system, encoders).encoders, ['libx264', 'aac', 'libwebp']);
    for (const changed of [{ webviewVersions: [] }, { webviewVersions: ['0.0.0.0'] }, { userInteractive: false }, { sessionId: 0 }]) assert.throws(() => validateHostedPrerequisites({ ...system, ...changed }, encoders));
});

test('all three actual fixture codecs must be listed by the host encoder', () => {
    assert.throws(() => validateHostedPrerequisites({ webviewVersions: ['152.0.4191.66'], userInteractive: true, sessionId: 1 }, encoders.replace('libwebp', 'missing')));
});

test('hosted proof is bound to the current job/source/attempt and finite freshness', () => {
    const context = hostedContext(environment, root, 'win32');
    const now = Date.parse('2026-10-07T23:00:00Z');
    const proof = { version: 1, kind: 'native-hosted-prerequisites', status: 'passed', checkedAt: new Date(now).toISOString(), context };
    assert.equal(validateHostedProof(proof, context, now), proof);
    assert.throws(() => validateHostedProof({ ...proof, status: 'failed' }, context, now));
    assert.throws(() => validateHostedProof(proof, { ...context, sourceCommit: 'b'.repeat(40) }, now));
    assert.throws(() => validateHostedProof(proof, { ...context, attempt: '2' }, now));
    assert.throws(() => validateHostedProof(proof, context, now + 3600001));
});

test('metadata failure categories distinguish deadline, missing executable, denied access, output bounds and early process exit', () => {
    const cases = [
        [{ killed: true, signal: 'SIGTERM' }, 10025, 'deadline'],
        [{ killed: true, signal: 'SIGTERM' }, 50, 'terminated'],
        [{ code: 'ENOENT' }, 5, 'executable_missing'],
        [{ code: 'EACCES' }, 5, 'access_denied'],
        [{ code: 'ERR_CHILD_PROCESS_STDIO_MAXBUFFER', killed: true }, 10025, 'output_limit'],
        [{ code: 7 }, 100, 'nonzero_exit'],
        [{ message: 'unknown private detail' }, 100, 'unclassified'],
    ];
    for (const [error, elapsed, category] of cases) {
        const proof = hostedMetadataFailure('C:/private/tools/pwsh.exe', 'system_graphics', error, elapsed, 'tonight-hosted:runtime_registry\ntonight-hosted:display\n');
        assert.equal(proof.category, category);
        assert.equal(proof.executable, 'pwsh.exe');
        assert.equal(proof.marker, 'display');
        assert.equal(proof.deadlineMs, 10000);
        assert.equal(proof.outputLimitBytes, 131072);
    }
});

test('safe metadata never retains raw paths, argv, environment, error messages or unrecognized categorical values', () => {
    const secret = 'private-bearer-and-argv-value';
    const error = { code: secret, signal: secret, message: secret, stdout: secret, stderr: secret, env: { TOKEN: secret }, args: [secret] };
    const proof = hostedMetadataFailure(`C:/private/${secret}.exe`, secret, error, Infinity, `${secret}\ntonight-hosted:${secret}\n`);
    assert.equal(proof.executable, 'other');
    assert.equal(proof.stage, 'metadata');
    assert.equal(proof.marker, null);
    assert.equal(proof.code, null);
    assert.equal(proof.signal, null);
    assert.equal(proof.elapsedMs, null);
    assert.equal(JSON.stringify(proof).includes(secret), false);
    assert.equal(Object.keys(proof).length, 10);
});

test('metadata reader preserves one ten-second bounded attempt and its sanitized failure', async () => {
    let calls = 0;
    let elapsed = 0;
    const reader = createHostedMetadataReader({ cwd: root, now: () => elapsed, execute: (executable, args, options, done) => {
        calls += 1;
        assert.equal(executable, 'pwsh.exe');
        assert.deepEqual(args, ['private-argv']);
        assert.equal(options.windowsHide, true);
        assert.equal(options.timeout, 10000);
        assert.equal(options.maxBuffer, 131072);
        elapsed = 10017;
        done({ killed: true, signal: 'SIGTERM', message: 'private-raw-error' }, 'private-stdout', 'private-stderr\ntonight-hosted:display\n');
    } });
    await assert.rejects(reader('pwsh.exe', ['private-argv'], 'system_graphics'), (error) => {
        assert.equal(error.metadataFailure.category, 'deadline');
        assert.equal(error.metadataFailure.elapsedMs, 10017);
        assert.equal(error.metadataFailure.marker, 'display');
        assert.equal(error.message, 'Hosted prerequisite metadata was unavailable from pwsh.exe.');
        assert.equal(JSON.stringify(error.metadataFailure).includes('private'), false);
        return true;
    });
    assert.equal(calls, 1);
});

test('metadata retains synchronous execution failures without raw details and success output stays parseable', async () => {
    const missing = createHostedMetadataReader({ cwd: root, execute: () => { throw Object.assign(new Error('private-command'), { code: 'ENOENT' }); } });
    await assert.rejects(missing('pwsh.exe', [], 'system_graphics'), (error) => error.metadataFailure.category === 'executable_missing' && !error.message.includes('private'));
    const success = createHostedMetadataReader({ cwd: root, execute: (_exe, _args, _options, done) => done(null, ' {"sessionId":2} \n', 'tonight-hosted:complete\n') });
    assert.deepEqual(JSON.parse(await success('pwsh.exe', [], 'system_graphics')), { sessionId: 2 });
});

test('an early probe failure is retained as a source-bound failed artifact before any later stage can run', async () => {
    const context = hostedContext(environment, root, 'win32');
    const directory = await mkdtemp(join(tmpdir(), 'duskcue-probe-evidence-'));
    const path = join(directory, 'hosted-ci-prerequisites.json');
    let attempts = 0;
    let later = false;
    const reader = createHostedMetadataReader({ cwd: root, now: () => 0, execute: (_exe, _args, _options, done) => { attempts += 1; done({ code: 2 }, '', 'tonight-hosted:runtime_registry\n'); } });
    try {
        const outcome = await retainHostedProbe(context, async () => {
            await reader('pwsh.exe', [], 'system_graphics');
            later = true;
            return {};
        }, (proof) => writeFile(path, `${JSON.stringify(proof)}\n`, { flag: 'wx' }), '2026-10-07T23:00:00.000Z');
        const artifact = JSON.parse(await readFile(path, 'utf8'));
        assert.equal(artifact.status, 'failed');
        assert.deepEqual(artifact.context, context);
        assert.equal(artifact.metadataFailure.category, 'nonzero_exit');
        assert.equal(artifact.metadataFailure.marker, 'runtime_registry');
        assert.ok(outcome.failure);
        assert.equal(attempts, 1);
        assert.equal(later, false);
        assert.equal(artifact.prerequisites, undefined);
        assert.equal(artifact.ffmpeg, undefined);
        assert.throws(() => validateHostedProof(artifact, context, Date.parse(artifact.checkedAt)));
    } finally { await unlink(path).catch(() => {}); await rmdir(directory); }
});
