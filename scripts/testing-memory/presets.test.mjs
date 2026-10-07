/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { PRESETS, workloadEnvironment, validateExtraArguments } from './presets.mjs';

test('child limits replace conflicting numeric and percentage heaps without changing parent settings', () => {
    const original = { NODE_OPTIONS: '--trace-warnings --max-old-space-size=8192 --max-old-space-size-percentage="80"', CARGO_BUILD_JOBS: '16' };
    const child = workloadEnvironment(PRESETS['web-unit'], original);
    assert.equal(child.NODE_OPTIONS, '--trace-warnings --max-old-space-size=1536');
    assert.equal(child.CARGO_BUILD_JOBS, '2');
    assert.equal(child.RUST_TEST_THREADS, '1');
    assert.equal(original.CARGO_BUILD_JOBS, '16');
    assert.ok(original.NODE_OPTIONS.includes('percentage'));
});

test('extra arguments cannot increase protected worker, Cargo or heap budgets', () => {
    for (const flag of ['--workers=4', '--maxWorkers', '--jobs=12', '-j16', '-j=16', '--test-threads=8', '--max-old-space-size-percentage=80']) {
        assert.throws(() => validateExtraArguments([flag]));
    }
    assert.doesNotThrow(() => validateExtraArguments(['playback-recovery.spec.ts', '--debug', '--config', 'qa.json']));
});
