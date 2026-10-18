// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import test from 'node:test';
import assert from 'node:assert/strict';
import { MetadataReadError } from './linux-engine-headroom.mjs';
import { createLinuxTelemetry } from './linux-telemetry.mjs';

const epoch = Date.parse('2026-10-17T23:00:00Z');
const sample = (age = 0, fields = {}) => ({ timestamp: new Date(epoch + age).toISOString(), context: 'desktop-linux', engineId: 'owned-engine', kernel: 'owned-kernel', bootId: 'owned-boot', totalBytes: 16 * 1024 ** 3, availableBytes: 8 * 1024 ** 3, ...fields });

test('metadata retry recovers within the original sample freshness and clears its error only after a valid sample', () => {
    let now = epoch + 2000;
    const telemetry = createLinuxTelemetry(sample(), { now: () => now });
    assert.equal(telemetry.readFailed(new MetadataReadError('wsl.exe')).allowed, true);
    assert.equal(telemetry.latest.timestamp, sample().timestamp);
    assert.match(telemetry.telemetryError, /wsl\.exe/);
    now = epoch + 4000;
    assert.equal(telemetry.observe(sample(4000)).allowed, true);
    assert.equal(telemetry.telemetryError, undefined);
    now = epoch + 12000;
    assert.equal(telemetry.check().allowed, true);
    now = epoch + 14001;
    assert.equal(telemetry.check().allowed, false);
});

test('persistent metadata errors never refresh the last sample and fail at the existing stale boundary with their reason', () => {
    let now = epoch;
    const telemetry = createLinuxTelemetry(sample(), { now: () => now });
    for (const age of [2000, 4000, 6000, 8000, 10000]) {
        now = epoch + age;
        assert.equal(telemetry.readFailed(new MetadataReadError('wsl.exe')).allowed, true);
        assert.equal(telemetry.latest.timestamp, sample().timestamp);
    }
    now = epoch + 10001;
    const decision = telemetry.check();
    assert.equal(decision.allowed, false);
    assert.match(decision.reason, /stale/);
    assert.match(decision.reason, /Read-only wsl\.exe metadata was unavailable/);
    assert.equal(telemetry.readFailed(new MetadataReadError('docker.exe')).allowed, false);
    assert.match(telemetry.telemetryError, /wsl\.exe/);
});

test('identity changes, malformed reads and real reserve loss are immediately ineligible for retry', () => {
    for (const fields of [{ engineId: 'foreign-engine' }, { bootId: 'new-boot' }, { timestamp: 'malformed' }, { availableBytes: 1024 ** 3 }, { availableBytes: Number.NaN }]) {
        const telemetry = createLinuxTelemetry(sample(), { now: () => epoch + 2000 });
        assert.equal(telemetry.readFailed(new MetadataReadError('wsl.exe')).allowed, true);
        assert.equal(telemetry.observe(sample(2000, fields)).allowed, false);
        assert.equal(telemetry.check().allowed, false);
    }
    for (const error of [new SyntaxError('malformed metadata JSON'), new Error('The local Docker engine changed during qualification.'), new Error('The docker-desktop WSL distribution is not already running; it will not be started by qualification.')]) {
        const telemetry = createLinuxTelemetry(sample(), { now: () => epoch + 2000 });
        assert.deepEqual(telemetry.readFailed(error), { allowed: false, reason: error.message });
        assert.equal(telemetry.telemetryError, undefined);
    }
});
