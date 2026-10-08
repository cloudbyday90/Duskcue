// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026 Duskcue Contributors
//
// This program is free software: licensed under AGPL-3.0
// See LICENSE file for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { awaitNativeFirstRun, isNativeAppDocument } from './qualification-startup.mjs';

function deferred() {
    let resolve;
    const promise = new Promise((done) => { resolve = done; });
    return { promise, resolve };
}

test('startup accepts only the actual Windows native application document origin', () => {
    for (const url of ['http://tauri.localhost/', 'https://tauri.localhost/auth/login']) assert.equal(isNativeAppDocument(url), true);
    for (const url of ['about:blank', 'http://127.0.0.1:48027/', 'http://tauri.localhost.evil/', 'http://tauri.localhost:5173/', 'http://user:secret@tauri.localhost/', 'invalid']) assert.equal(isNativeAppDocument(url), false);
});

test('a CDP-ready blank page cannot advance to gate or native bridge checks before navigation commits', async () => {
    const navigation = deferred();
    const gate = deferred();
    const calls = [];
    let url = 'about:blank';
    const page = {
        async waitForURL(predicate, options) { calls.push('navigation'); assert.equal(options.waitUntil, 'domcontentloaded'); assert.equal(predicate(new URL(url)), false); await navigation.promise; assert.equal(predicate(new URL(url)), true); },
        getByRole(role, options) { assert.equal(role, 'heading'); assert.equal(options.name, 'Connect to your server'); return { async waitFor(options) { calls.push('gate'); assert.equal(options.state, 'visible'); await gate.promise; } }; },
        getByLabel(name) { assert.equal(name, 'Server URL'); return { async waitFor() { calls.push('field'); } }; },
        async waitForFunction() { calls.push('bridge'); },
        url: () => url,
    };
    const ready = awaitNativeFirstRun(page);
    await Promise.resolve();
    assert.deepEqual(calls, ['navigation']);
    url = 'http://tauri.localhost/';
    navigation.resolve();
    await Promise.resolve();
    await Promise.resolve();
    assert.deepEqual(calls, ['navigation', 'gate']);
    gate.resolve();
    assert.equal((await ready).readyNativeBridge, true);
    assert.deepEqual(calls, ['navigation', 'gate', 'field', 'bridge']);
});

test('startup shares one finite deadline across navigation, UI and bridge rather than extending each phase', async () => {
    let time = 0;
    const timeouts = [];
    const advance = async (options) => { timeouts.push(options.timeout); time += 8; };
    const page = {
        async waitForURL(predicate, options) { await advance(options); },
        getByRole: () => ({ waitFor: advance }),
        getByLabel: () => ({ waitFor: advance }),
        async waitForFunction() { throw new Error('Bridge wait should not begin after the deadline.'); },
        url: () => 'http://tauri.localhost/',
    };
    await assert.rejects(awaitNativeFirstRun(page, { timeoutMs: 20, now: () => time }), /exceeded its deadline/);
    assert.deepEqual(timeouts, [20, 12, 4]);
});

test('leaving the native origin before isolation remains a failure', async () => {
    const page = {
        async waitForURL() {}, getByRole: () => ({ async waitFor() {} }), getByLabel: () => ({ async waitFor() {} }), async waitForFunction() {},
        url: () => 'http://unexpected.example/',
    };
    await assert.rejects(awaitNativeFirstRun(page), /document changed/);
});
