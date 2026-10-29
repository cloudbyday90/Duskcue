// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { runBounded } from '../qualification/commands.mjs';
import { redactAndroidOutput, validateAndroidHardware } from './android-evidence.mjs';

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
            assert.ok(output.includes('[redacted-relay-url]'));
        }
    } finally { await rm(directory, { recursive: true }); }
});
