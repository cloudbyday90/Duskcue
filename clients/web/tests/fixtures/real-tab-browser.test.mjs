/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import { appendFile, mkdtemp, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';
import { launchRealTabBrowser, parseDevToolsActivePort } from './real-tab-browser.mjs';

const portFile = '9222\n/devtools/browser/12345678-1234-1234-1234-123456789abc\n';

async function fixture(options = {}) {
    const profile = await mkdtemp(join(tmpdir(), 'playwright_chromiumdev_profile-unit-'));
    const portPath = join(profile, 'DevToolsActivePort');
    let publishing = Promise.resolve();
    if (options.growingPort) {
        await writeFile(portPath, '');
        publishing = (async () => {
            await new Promise((resolve) => setTimeout(resolve, 30));
            await appendFile(portPath, '9222\n/devtools/browser/');
            await new Promise((resolve) => setTimeout(resolve, 30));
            await appendFile(portPath, '12345678-1234-1234-1234-123456789abc');
        })();
    } else if (!options.missingPort) await writeFile(portPath, options.portFile ?? portFile);
    const calls = [];
    const child = Object.assign(new EventEmitter(), { pid: 12345, exitCode: null, signalCode: null, spawnargs: ['chrome', `--user-data-dir=${profile}`, ...(options.arguments ?? [])] });
    const exit = (code, signal) => { child.exitCode = code; child.signalCode = signal; child.emit('exit', code, signal); };
    const context = {};
    const browser = { contexts: () => options.contexts ?? [context], close: async () => { calls.push(['disconnect']); } };
    const server = {
        process: () => child,
        close: async () => {
            calls.push(['close']);
            if (options.closeHangs) return new Promise(() => {});
            if (options.closeFails) throw new Error('close failed');
            if (!options.neverExits) {
                if (options.exitDelayMs) setTimeout(() => exit(0, null), options.exitDelayMs);
                else exit(0, null);
            }
        },
        kill: async () => { calls.push(['kill']); if (!options.neverExits) exit(null, 'SIGKILL'); },
    };
    const chromium = {
        launchServer: async (options) => { calls.push(['launch', options]); return server; },
        connectOverCDP: async (endpoint, settings) => {
            calls.push(['connect', endpoint, settings]);
            if (options.connectFails) throw new Error('connect failed');
            return browser;
        },
    };
    return { chromium, calls, child, context, remove: async () => {
        await publishing;
        assert.equal(dirname(resolve(profile)), resolve(tmpdir()));
        assert(basename(profile).startsWith('playwright_chromiumdev_profile-unit-'));
        await rm(profile, { recursive: true, force: true });
    } };
}

test('DevTools endpoint accepts only a bounded loopback browser UUID and real port', () => {
    assert.equal(parseDevToolsActivePort(portFile), 'ws://127.0.0.1:9222/devtools/browser/12345678-1234-1234-1234-123456789abc');
    for (const invalid of ['0\n/devtools/browser/12345678-1234-1234-1234-123456789abc', portFile.replace('9222', '65536'), portFile.replace('/devtools/browser/', '/other/'), `${portFile}extra`, 'x'.repeat(4097)]) assert.throws(() => parseDevToolsActivePort(invalid));
});

test('headed default-context ownership uses noDefaults and disposes once across concurrent callers', async () => {
    const fake = await fixture();
    try {
        const owned = await launchRealTabBrowser(fake.chromium);
        assert.equal(owned.context, fake.context);
        assert.deepEqual(fake.calls[0], ['launch', { headless: false, args: ['--remote-debugging-port=0'], host: '127.0.0.1', timeout: 10000 }]);
        assert.deepEqual(fake.calls[1][2], { noDefaults: true, isLocal: true, timeout: 10000 });
        await Promise.all([owned.dispose(), owned.dispose()]);
        assert.equal(fake.calls.filter(([name]) => name === 'disconnect').length, 1);
        assert.equal(fake.calls.filter(([name]) => name === 'close').length, 1);
        assert.equal(fake.calls.filter(([name]) => name === 'kill').length, 0);
        assert.deepEqual(owned.proof.cleanup, { passed: true, exited: true, killFallbackUsed: false, exitCode: 0, signalCode: null });
    } finally { await fake.remove(); }
});

test('failed attachment closes the originally owned browser before propagating the setup error', async () => {
    const fake = await fixture({ connectFails: true });
    try {
        await assert.rejects(launchRealTabBrowser(fake.chromium), /connect failed/);
        assert.equal(fake.child.exitCode, 0);
        assert.equal(fake.calls.filter(([name]) => name === 'close').length, 1);
    } finally { await fake.remove(); }
});

test('failed graceful cleanup still kills the owned process and retains the failure', async () => {
    const fake = await fixture({ closeFails: true });
    try {
        const owned = await launchRealTabBrowser(fake.chromium);
        await assert.rejects(owned.dispose(), /cleanup failed/);
        assert.deepEqual(owned.proof.cleanup, { passed: false, exited: true, killFallbackUsed: true, exitCode: null, signalCode: 'SIGKILL' });
    } finally { await fake.remove(); }
});

test('hung graceful cleanup reaches the bounded kill fallback without claiming a clean shutdown', async () => {
    const fake = await fixture({ closeHangs: true });
    try {
        const owned = await launchRealTabBrowser(fake.chromium, { timeoutMs: 10 });
        await assert.rejects(owned.dispose(), /cleanup failed/);
        assert.equal(owned.proof.cleanup.exited, true);
        assert.equal(owned.proof.cleanup.passed, false);
        assert.equal(owned.proof.cleanup.killFallbackUsed, true);
    } finally { await fake.remove(); }
});

test('unexpected context inventory cannot silently replace the required existing default context', async () => {
    const fake = await fixture({ contexts: [{}, {}] });
    try {
        await assert.rejects(launchRealTabBrowser(fake.chromium), /sole existing default/);
        assert.equal(fake.child.exitCode, 0);
        assert(fake.calls.some(([name]) => name === 'disconnect'));
    } finally { await fake.remove(); }
});

test('graceful API completion waits for the originally owned child physical exit', async () => {
    const fake = await fixture({ exitDelayMs: 25 });
    try {
        const owned = await launchRealTabBrowser(fake.chromium);
        let completed = false;
        const disposal = owned.dispose().then(() => { completed = true; });
        await new Promise((resolve) => setTimeout(resolve, 5));
        assert.equal(completed, false);
        assert.equal(fake.child.exitCode, null);
        await disposal;
        assert.equal(fake.child.exitCode, 0);
        assert.equal(owned.proof.cleanup.passed, true);
    } finally { await fake.remove(); }
});

test('a successful kill API acknowledgment cannot qualify an unconfirmed physical exit', async () => {
    const fake = await fixture({ neverExits: true });
    try {
        const owned = await launchRealTabBrowser(fake.chromium, { timeoutMs: 10 });
        await assert.rejects(owned.dispose(), /cleanup failed/);
        assert.equal(owned.proof.cleanup.exited, false);
        assert.equal(owned.proof.cleanup.passed, false);
        assert.equal(owned.proof.cleanup.killFallbackUsed, true);
        assert.equal(fake.child.listenerCount('exit'), 0);
    } finally { await fake.remove(); }
});

test('headless invocation or oversized port file is rejected before attachment with owned cleanup', async () => {
    for (const options of [{ arguments: ['--headless=new'] }, { portFile: 'x'.repeat(4097) }]) {
        const fake = await fixture(options);
        try {
            await assert.rejects(launchRealTabBrowser(fake.chromium));
            assert.equal(fake.child.exitCode, 0);
            assert.equal(fake.calls.filter(([name]) => name === 'connect').length, 0);
        } finally { await fake.remove(); }
    }
});

test('missing CDP readiness reaches its deadline and releases the originally owned browser', async () => {
    const fake = await fixture({ missingPort: true });
    try {
        await assert.rejects(launchRealTabBrowser(fake.chromium, { timeoutMs: 10 }), /readiness exceeded its deadline/);
        assert.equal(fake.child.exitCode, 0);
        assert.equal(fake.calls.filter(([name]) => name === 'connect').length, 0);
    } finally { await fake.remove(); }
});

test('a live producer may publish an empty and partial port file without attaching before its final UUID', async () => {
    const fake = await fixture({ growingPort: true });
    try {
        const owned = await launchRealTabBrowser(fake.chromium);
        assert.equal(fake.calls.filter(([name]) => name === 'connect').length, 1);
        assert.equal(fake.calls.find(([name]) => name === 'connect')[1], parseDevToolsActivePort(portFile));
        await owned.dispose();
    } finally { await fake.remove(); }
});
