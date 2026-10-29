// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, realpath } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, basename } from 'node:path';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput, validateAndroidHardware, validateAndroidAvd, installedAndroidApk, androidFailure } from './android-evidence.mjs';

test('nested failures remain bounded and redact both media and VM-service capabilities', () => {
    const secret = 'private-token';
    const failure = new AggregateError([new Error(`${secret} http://127.0.0.1:1234/Le4dZ3hGBak=/ws`) ], 'cleanup failed');
    const serialized = JSON.stringify(androidFailure(failure, secret));
    assert.ok(serialized.includes('cleanup failed'));
    assert.ok(!serialized.includes(secret)); assert.ok(!serialized.includes('Le4dZ3hGBak='));
    assert.ok(serialized.length < 4096);
});

test('installed package proof accepts Android randomized directories and rejects path or package substitution', () => {
    const path = '/data/app/~~Random_+=/com.duskcue.mobile.qualification-suffix==/base.apk';
    assert.equal(installedAndroidApk(`package:${path}`, 'com.duskcue.mobile.qualification'), path);
    for (const response of [`package:${path}\npackage:/other`, 'package:/data/app/../com.duskcue.mobile.qualification-suffix/base.apk',
        'package:/data/app/random/com.other.app-suffix/base.apk', `package:${path};echo injected`]) {
        assert.throws(() => installedAndroidApk(response, 'com.duskcue.mobile.qualification'));
    }
});

test('AVD console framing retains exact identity and refuses other or malformed devices', () => {
    for (const response of ['owned\nOK\n', 'owned\r\r\nOK\r\r\n']) {
        assert.doesNotThrow(() => validateAndroidAvd(response, 'owned'));
    }
    for (const response of ['other\nOK', 'owned-other\nOK', 'OK\nowned', 'owned\nother\nOK', 'owned\nOK\nOK', 'owned']) {
        assert.throws(() => validateAndroidAvd(response, 'owned'));
    }
});

test('actual generated hardware must match the approved budget, including emulator auto-increases', () => {
    assert.doesNotThrow(() => validateAndroidHardware('hw.ramSize = 2048\nhw.cpu.ncore = 2\n', 2048));
    for (const actual of ['hw.ramSize = 2560\nhw.cpu.ncore = 2\n', 'hw.ramSize = 2048\nhw.cpu.ncore = 4\n',
        'hw.ramSize = 2048\n', 'hw.ramSize = 2048\nhw.ramSize = 2560\nhw.cpu.ncore = 2\n']) {
        assert.throws(() => validateAndroidHardware(actual, 2048));
    }
});

test('split native output is redacted before evidence persistence and returned diagnostics', async () => {
    const directory = await mkdtemp(join(tmpdir(), 'duskcue-android-evidence-'));
    const log = join(directory, 'native.log');
    const capability = 'a'.repeat(43);
    try {
        const result = await runBounded(process.execPath, ['-e',
            `process.stdout.write('Bearer private-token http://127.0.0.1:12345/${capability.slice(0, 20)}'); setTimeout(() => process.stdout.write('${capability.slice(20)}/v/index.m3u8'), 20)`],
        { log, redact: ['private-token'], sanitize: redactAndroidOutput, timeoutMs: 5000 });
        assert.equal(result.code, 0);
        for (const output of [result.stdout, await readFile(log, 'utf8')]) {
            assert.ok(!output.includes(capability)); assert.ok(!output.includes('private-token'));
            assert.ok(output.includes('[redacted-local-capability-url]'));
        }
    } finally {
        const target = await realpath(directory);
        assert.equal(dirname(target), await realpath(tmpdir()));
        assert.ok(basename(target).startsWith('duskcue-android-evidence-'));
        await rm(target, { recursive: true });
    }
});
