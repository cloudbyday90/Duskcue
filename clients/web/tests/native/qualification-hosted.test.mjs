// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { hostedContext, validateHostedPrerequisites, validateHostedProof } from './qualification-hosted.mjs';

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
