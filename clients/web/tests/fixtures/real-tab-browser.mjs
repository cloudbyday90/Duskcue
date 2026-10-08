/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { constants } from 'node:fs';
import { lstat, open } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { tmpdir } from 'node:os';

export function parseDevToolsActivePort(text) {
    if (typeof text !== 'string' || Buffer.byteLength(text) > 4096) throw new Error('Invalid DevTools port file size');
    const lines = text.trimEnd().split(/\r?\n/);
    if (lines.length !== 2 || !/^[1-9]\d{0,4}$/.test(lines[0]) || Number(lines[0]) > 65535 || !/^\/devtools\/browser\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(lines[1])) throw new Error('Invalid DevTools port file contents');
    return `ws://127.0.0.1:${lines[0]}${lines[1]}`;
}

function incompletePortFile(text) {
    const lines = text.split('\n');
    if (lines.length > 2 || !/^[1-9]\d{0,4}$/.test(lines[0]) || Number(lines[0]) > 65535) return false;
    if (lines.length === 1) return true;
    const prefix = '/devtools/browser/';
    if (prefix.startsWith(lines[1])) return true;
    if (!lines[1].startsWith(prefix)) return false;
    const uuid = lines[1].slice(prefix.length);
    const format = 'xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx';
    return uuid.length < format.length && [...uuid].every((character, index) => format[index] === '-' ? character === '-' : /^[0-9a-f]$/.test(character));
}

async function readEndpoint(profile) {
    const path = join(profile, 'DevToolsActivePort');
    const before = await lstat(path);
    if (!before.isFile() || before.isSymbolicLink() || before.size > 4096) throw new Error('DevTools port file must be a bounded regular file');
    if (before.size === 0) return undefined;
    const handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) | (constants.O_NONBLOCK ?? 0));
    try {
        const stat = await handle.stat();
        if (!stat.isFile() || stat.size < before.size || stat.size > 4096 || stat.ino !== before.ino || stat.dev !== before.dev) throw new Error('DevTools port file changed before reading');
        if (stat.size !== before.size) return undefined;
        const bytes = Buffer.alloc(stat.size);
        const read = await handle.read(bytes, 0, bytes.length, 0);
        if (read.bytesRead !== bytes.length) throw new Error('Incomplete DevTools port file');
        const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
        try { return parseDevToolsActivePort(text); } catch (error) { if (incompletePortFile(text)) return undefined; throw error; }
    } finally { await handle.close(); }
}

async function bounded(promise, milliseconds, label) {
    let timer;
    try {
        return await Promise.race([promise, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(`${label} exceeded its deadline`)), milliseconds); })]);
    } finally { clearTimeout(timer); }
}

export async function launchRealTabBrowser(chromium, { timeoutMs = 10_000 } = {}) {
    if (!Number.isInteger(timeoutMs) || timeoutMs < 10 || timeoutMs > 15_000) throw new Error('Invalid real-tab browser deadline');
    const server = await chromium.launchServer({ headless: false, args: ['--remote-debugging-port=0'], host: '127.0.0.1', timeout: timeoutMs });
    const child = server.process();
    const proof = { noDefaults: true, pid: child.pid, headlessArguments: child.spawnargs.filter((arg) => /^--headless(?:=|$)/.test(arg)), cleanup: { passed: false, exited: false, killFallbackUsed: false, exitCode: null, signalCode: null } };
    let browser;
    let disposal;
    const exited = () => child.exitCode !== null || child.signalCode !== null;
    const waitForExit = () => {
        if (exited()) return Promise.resolve();
        return new Promise((resolve, reject) => {
            const complete = () => { clearTimeout(timer); child.removeListener('exit', complete); resolve(); };
            const timer = setTimeout(() => { child.removeListener('exit', complete); reject(new Error('Owned browser process exit exceeded its deadline')); }, timeoutMs);
            child.once('exit', complete);
            if (exited()) complete();
        });
    };
    const dispose = () => disposal ??= (async () => {
        const failures = [];
        if (browser) {
            try { await bounded(browser.close(), timeoutMs, 'CDP disconnect'); } catch (error) { failures.push(error); }
        }
        try { await bounded(Promise.all([server.close(), waitForExit()]), timeoutMs, 'Owned browser close'); } catch (error) { failures.push(error); }
        if (!exited()) {
            proof.cleanup.killFallbackUsed = true;
            try { await bounded(Promise.all([server.kill(), waitForExit()]), timeoutMs, 'Owned browser kill'); } catch (error) { failures.push(error); }
        }
        proof.cleanup.exited = exited();
        proof.cleanup.exitCode = child.exitCode;
        proof.cleanup.signalCode = child.signalCode;
        if (!proof.cleanup.exited) failures.push(new Error('Owned browser process exit was not confirmed'));
        proof.cleanup.passed = failures.length === 0;
        if (failures.length) throw new AggregateError(failures, 'Real-tab browser cleanup failed');
    })();
    try {
        if (!Number.isInteger(child.pid) || child.pid <= 0 || proof.headlessArguments.length) throw new Error('Real-tab browser must have an owned headed process');
        const profiles = child.spawnargs.filter((arg) => arg.startsWith('--user-data-dir='));
        if (profiles.length !== 1) throw new Error('Expected one library-owned browser profile');
        const profile = profiles[0].slice('--user-data-dir='.length);
        if (!isAbsolute(profile) || dirname(resolve(profile)) !== resolve(tmpdir()) || !basename(profile).startsWith('playwright_chromiumdev_profile-')) throw new Error('Unexpected browser profile ownership');
        const profileStat = await bounded(lstat(profile), timeoutMs, 'Owned browser profile read');
        if (!profileStat.isDirectory() || profileStat.isSymbolicLink()) throw new Error('Browser profile must be the owned regular directory');
        const until = performance.now() + timeoutMs;
        let endpoint;
        while (!endpoint) {
            if (exited()) throw new Error('Owned browser exited before CDP readiness');
            if (performance.now() >= until) throw new Error('DevTools readiness exceeded its deadline');
            try { endpoint = await bounded(readEndpoint(profile), Math.max(1, until - performance.now()), 'DevTools port read'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
            if (!endpoint) await new Promise((resolve) => setTimeout(resolve, 25));
        }
        browser = await chromium.connectOverCDP(endpoint, { noDefaults: true, isLocal: true, timeout: timeoutMs });
        const contexts = browser.contexts();
        if (contexts.length !== 1) throw new Error('Expected the sole existing default browser context');
        return { context: contexts[0], browser, proof, dispose };
    } catch (error) {
        try { await dispose(); } catch (cleanupError) { throw new AggregateError([error, cleanupError], 'Real-tab browser setup and cleanup failed'); }
        throw error;
    }
}
