/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import test from 'node:test';
import { GiB, evaluateEmergency, evaluatePreflight } from './policy.mjs';

const nowMs = Date.parse('2026-10-07T12:00:00.000Z');
const options = { nowMs };
const sample = (changes = {}) => ({
    timestamp: new Date(nowMs).toISOString(),
    totalPhysicalBytes: 32 * GiB,
    availablePhysicalBytes: 8 * GiB,
    committedBytes: 40 * GiB,
    commitLimitBytes: 64 * GiB,
    processes: [{ pid: 100, parentPid: 1, name: 'node', privateBytes: GiB, workingSetBytes: GiB / 2 }],
    ...changes
});
const codes = (result) => result.reasons.map((reason) => reason.code);

test('every workload accepts a healthy current sample', () => {
    for (const workload of ['unit', 'browser', 'build', 'native', 'cargo']) {
        assert.equal(evaluatePreflight(sample(), workload, options).allowed, true);
    }
});

test('available physical memory is an independent workload gate', () => {
    for (const [workload, reserve] of [['unit', 2], ['browser', 3], ['build', 4], ['native', 4], ['cargo', 4]]) {
        assert.equal(evaluatePreflight(sample({ availablePhysicalBytes: reserve * GiB }), workload, options).allowed, true);
        assert.deepEqual(codes(evaluatePreflight(sample({ availablePhysicalBytes: reserve * GiB - 1 }), workload, options)), ['LOW_PHYSICAL_MEMORY']);
    }
});

test('commit headroom is an independent gate at each workload reserve', () => {
    for (const [workload, reserve] of [['unit', 2], ['browser', 8], ['build', 12], ['native', 12], ['cargo', 12]]) {
        const commitLimitBytes = 16 * GiB;
        const committedBytes = commitLimitBytes - reserve * GiB;
        assert.equal(evaluatePreflight(sample({ commitLimitBytes, committedBytes }), workload, options).allowed, true);
        assert.deepEqual(codes(evaluatePreflight(sample({ commitLimitBytes, committedBytes: committedBytes + 1 }), workload, options)), ['LOW_COMMIT_HEADROOM']);
    }
});

test('commit percentage is independent from ample physical and commit headroom', () => {
    for (const [workload, percent] of [['unit', 92], ['browser', 85], ['build', 85], ['native', 85], ['cargo', 85]]) {
        const commitLimitBytes = 100 * GiB;
        const committedBytes = percent * GiB;
        assert.equal(evaluatePreflight(sample({ commitLimitBytes, committedBytes }), workload, options).allowed, true);
        assert.deepEqual(codes(evaluatePreflight(sample({ commitLimitBytes, committedBytes: committedBytes + 1 }), workload, options)), ['HIGH_COMMIT_PERCENT']);
    }
});

test('all failed gates are reported without changing the input', () => {
    const input = sample({ availablePhysicalBytes: GiB, committedBytes: 63 * GiB });
    const before = structuredClone(input);
    assert.deepEqual(codes(evaluatePreflight(input, 'build', options)), ['LOW_PHYSICAL_MEMORY', 'LOW_COMMIT_HEADROOM', 'HIGH_COMMIT_PERCENT']);
    assert.deepEqual(input, before);
});

test('emergency physical and headroom thresholds are strict, percentage is inclusive', () => {
    assert.equal(evaluateEmergency(sample({ availablePhysicalBytes: 1.5 * GiB, commitLimitBytes: 16 * GiB, committedBytes: 14 * GiB }), options).allowed, true);
    assert.deepEqual(codes(evaluateEmergency(sample({ availablePhysicalBytes: 1.5 * GiB - 1 }), options)), ['LOW_PHYSICAL_MEMORY']);
    assert.deepEqual(codes(evaluateEmergency(sample({ commitLimitBytes: 16 * GiB, committedBytes: 14 * GiB + 1 }), options)), ['LOW_COMMIT_HEADROOM']);
    assert.deepEqual(codes(evaluateEmergency(sample({ commitLimitBytes: 100 * GiB, committedBytes: 94 * GiB }), options)), ['HIGH_COMMIT_PERCENT']);
});

test('missing, failed and malformed measurements fail closed for preflight and running jobs', () => {
    const invalid = [null, undefined, [], {}, sample({ error: { code: 'MEMORY_MEASUREMENT_FAILED' } }), sample({ committedBytes: NaN }), sample({ commitLimitBytes: 0 }), sample({ availablePhysicalBytes: -1 }), sample({ availablePhysicalBytes: 33 * GiB }), sample({ totalPhysicalBytes: '32' }), sample({ committedBytes: Infinity }), sample({ committedBytes: Number.MAX_SAFE_INTEGER + 1 }), sample({ timestamp: 'invalid' }), sample({ processes: null }), sample({ processes: [{ pid: 1 }] })];
    for (const value of invalid) {
        assert.equal(evaluatePreflight(value, 'unit', options).allowed, false);
        assert.equal(evaluateEmergency(value, options).allowed, false);
    }
});

test('lost or stale samples stop authorizing work after ten seconds', () => {
    assert.equal(evaluateEmergency(sample({ timestamp: new Date(nowMs - 10000).toISOString() }), options).allowed, true);
    assert.deepEqual(codes(evaluatePreflight(sample({ timestamp: new Date(nowMs - 10001).toISOString() }), 'unit', options)), ['STALE_MEASUREMENT']);
    assert.deepEqual(codes(evaluateEmergency(sample({ timestamp: new Date(nowMs - 10001).toISOString() }), options)), ['STALE_MEASUREMENT']);
});

test('future timestamps and invalid evaluation clocks fail closed', () => {
    assert.deepEqual(codes(evaluateEmergency(sample({ timestamp: new Date(nowMs + 5001).toISOString() }), options)), ['FUTURE_MEASUREMENT']);
    assert.deepEqual(codes(evaluateEmergency(sample(), { nowMs: NaN })), ['INVALID_EVALUATION_CLOCK']);
    assert.deepEqual(codes(evaluateEmergency(sample(), { ...options, maxSampleAgeMs: -1 })), ['INVALID_EVALUATION_CLOCK']);
    assert.deepEqual(codes(evaluateEmergency(sample(), null)), ['INVALID_EVALUATION_CLOCK']);
});

test('over-limit commit remains a real emergency and unknown workloads are denied', () => {
    const result = evaluateEmergency(sample({ committedBytes: 65 * GiB }), options);
    assert.equal(result.metrics.commitHeadroomBytes, -GiB);
    assert.deepEqual(codes(result), ['LOW_COMMIT_HEADROOM', 'HIGH_COMMIT_PERCENT']);
    assert.deepEqual(codes(evaluatePreflight(sample(), 'unknown', options)), ['UNKNOWN_WORKLOAD']);
    assert.deepEqual(codes(evaluatePreflight(sample(), '__proto__', options)), ['UNKNOWN_WORKLOAD']);
});
