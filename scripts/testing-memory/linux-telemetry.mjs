// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { MetadataReadError, evaluateLinuxHeadroom } from './linux-engine-headroom.mjs';

export function createLinuxTelemetry(initialSample, { now = Date.now } = {}) {
    let latest = initialSample;
    let telemetryError;
    const decision = (sample, preflight) => {
        const current = evaluateLinuxHeadroom(sample, initialSample, { nowMs: now(), preflight });
        return !current.allowed && telemetryError ? { ...current, reason: `${current.reason} Last metadata read error: ${telemetryError}` } : current;
    };
    return {
        get latest() { return latest; },
        get telemetryError() { return telemetryError; },
        check({ preflight = false } = {}) { return decision(latest, preflight); },
        observe(sample, { preflight = false } = {}) {
            latest = sample;
            const current = decision(sample, preflight);
            if (current.allowed) telemetryError = undefined;
            return current;
        },
        readFailed(error) {
            if (!(error instanceof MetadataReadError)) return { allowed: false, reason: error?.message || String(error) };
            telemetryError ||= error.message;
            return decision(latest, false);
        },
    };
}
