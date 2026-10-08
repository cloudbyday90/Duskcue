// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { assertOwnedNativeWindow, requireHostedWindowContext } from './qualification-window-control-policy.mjs';
import { assertBackgroundEvidence, countdownSeconds } from './qualification-background-policy.mjs';

const root = 'C:/hosted/Duskcue';
const environment = { GITHUB_ACTIONS: 'true', RUNNER_ENVIRONMENT: 'github-hosted', RUNNER_OS: 'Windows', GITHUB_WORKSPACE: root, GITHUB_SHA: 'a'.repeat(40), GITHUB_REPOSITORY: 'cloudbyday90/Duskcue', GITHUB_RUN_ID: '123', GITHUB_RUN_ATTEMPT: '1', GITHUB_JOB: 'native-windows' };
const manifest = { hostedCI: { context: { sourceCommit: environment.GITHUB_SHA, runId: '123', attempt: '1', job: 'native-windows' } } };
const host = { pid: 10, parentPid: 20, name: 'duskcue-tonight-qualification.exe', path: 'C:/hosted/Duskcue/target/debug/duskcue-tonight-qualification.exe', createdAt: '2026-10-08T00:00:00.0000000Z', sha256: 'a'.repeat(64) };
const hidden = { visibility: 'hidden', hidden: true, cardFocused: false, menuOpen: false, text: 'Countdown paused at 8s', path: '/play/episode', events: [{ visibility: 'hidden', trusted: true }] };

test('PowerShell identity JSON keeps the exact seven-fraction creation timestamp and distinguishes one tick', { skip: process.platform !== 'win32' }, async () => {
    const script = `$json = '{"createdAt":"2026-10-08T01:37:15.9117940Z","different":"2026-10-08T01:37:15.9117941Z"}'; $parsed = $json | ConvertFrom-Json -DateKind String; [pscustomobject]@{type=$parsed.createdAt.GetType().FullName; value=$parsed.createdAt; different=$parsed.different; distinct=($parsed.createdAt -cne $parsed.different)} | ConvertTo-Json -Compress`;
    const response = await promisify(execFile)('pwsh.exe', ['-NoProfile', '-Command', script], { windowsHide: true, timeout: 5000, maxBuffer: 4096 });
    const parsed = JSON.parse(response.stdout.trim());
    assert.equal(parsed.type, 'System.String');
    assert.equal(parsed.value, '2026-10-08T01:37:15.9117940Z');
    assert.equal(parsed.different, '2026-10-08T01:37:15.9117941Z');
    assert.equal(parsed.distinct, true);
    const exact = { ...host, createdAt: parsed.value };
    assert.equal(assertOwnedNativeWindow(exact, exact, ['101']), '101');
    assert.throws(() => assertOwnedNativeWindow(exact, { ...exact, createdAt: parsed.different }, ['101']));
});

test('window control cannot target local/self-hosted/windows from another native job', () => {
    assert.equal(requireHostedWindowContext(environment, root, manifest, 'win32').job, 'native-windows');
    for (const change of [{ GITHUB_ACTIONS: undefined }, { RUNNER_ENVIRONMENT: 'self-hosted' }, { GITHUB_SHA: 'b'.repeat(40) }, { GITHUB_RUN_ATTEMPT: '2' }]) assert.throws(() => requireHostedWindowContext({ ...environment, ...change }, root, manifest, 'win32'));
    assert.throws(() => requireHostedWindowContext(environment, root, manifest, 'linux'));
    assert.throws(() => requireHostedWindowContext(environment, root, {}, 'win32'));
});

test('PID reuse, changed parent/name/path/creation or SHA denies every window action', () => {
    assert.equal(assertOwnedNativeWindow(host, { ...host }, ['101']), '101');
    for (const change of [{ pid: 11 }, { parentPid: 21 }, { name: 'user-app.exe' }, { path: 'C:/other/duskcue-tonight-qualification.exe' }, { createdAt: '2026-10-08T00:01:00.0000000Z' }, { sha256: 'b'.repeat(64) }]) assert.throws(() => assertOwnedNativeWindow(host, { ...host, ...change }, ['101']));
    assert.throws(() => assertOwnedNativeWindow({ ...host, name: 'user-app.exe' }, host, ['101']));
});

test('HWND must be unique and match its original pin on minimize, restore and finally restore', () => {
    const pinned = { ...host, handle: '101' };
    assert.equal(assertOwnedNativeWindow(pinned, host, ['101']), '101');
    for (const handles of [[], ['101', '102'], ['102'], ['0'], [101]]) assert.throws(() => assertOwnedNativeWindow(pinned, host, handles));
});

test('real background evidence rejects minimized-only or synthetic visibility claims', () => {
    assert.doesNotThrow(() => assertBackgroundEvidence(hidden, { ...hidden }, 1, 1));
    for (const change of [{ visibility: 'visible' }, { hidden: false }, { events: [] }, { events: [{ visibility: 'hidden', trusted: false }] }]) assert.throws(() => assertBackgroundEvidence({ ...hidden, ...change }, hidden, 1, 1));
});

test('card focus or disclosure cannot mask failure of the actual document-hidden pause', () => {
    assert.throws(() => assertBackgroundEvidence({ ...hidden, cardFocused: true }, hidden, 1, 1));
    assert.throws(() => assertBackgroundEvidence(hidden, { ...hidden, menuOpen: true }, 1, 1));
});

test('background countdown text, playback starts and current episode must stay fixed', () => {
    assert.throws(() => assertBackgroundEvidence(hidden, { ...hidden, text: 'Countdown paused at 7s' }, 1, 1));
    assert.throws(() => assertBackgroundEvidence(hidden, hidden, 1, 2));
    assert.throws(() => assertBackgroundEvidence(hidden, { ...hidden, path: '/play/next' }, 1, 1));
    assert.equal(countdownSeconds(hidden), 8);
    assert.throws(() => countdownSeconds({ text: 'Starting the next episode' }));
});
