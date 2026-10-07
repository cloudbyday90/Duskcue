/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export const GiB = 1024 ** 3;

const buildPolicy = Object.freeze({ minAvailableBytes: 4 * GiB, minCommitHeadroomBytes: 12 * GiB, maxCommitPercent: 85 });

export const WORKLOAD_POLICIES = Object.freeze({
    unit: Object.freeze({ minAvailableBytes: 2 * GiB, minCommitHeadroomBytes: 2 * GiB, maxCommitPercent: 92 }),
    browser: Object.freeze({ minAvailableBytes: 3 * GiB, minCommitHeadroomBytes: 8 * GiB, maxCommitPercent: 85 }),
    check: Object.freeze({ minAvailableBytes: 4 * GiB, minCommitHeadroomBytes: 8 * GiB, maxCommitPercent: 85 }),
    build: buildPolicy,
    native: buildPolicy,
    cargo: buildPolicy
});

export const EMERGENCY_POLICY = Object.freeze({ minAvailableBytes: 1.5 * GiB, minCommitHeadroomBytes: 2 * GiB, maxCommitPercent: 94 });

function reason(code, message, actual, limit) {
    return { code, message, ...(actual === undefined ? {} : { actual }), ...(limit === undefined ? {} : { limit }) };
}

function validateSample(sample, options = {}) {
    if (!options || typeof options !== 'object' || Array.isArray(options)) {
        return { allowed: false, reasons: [reason('INVALID_EVALUATION_CLOCK', 'The measurement freshness limits are invalid.')] };
    }
    const { nowMs = Date.now(), maxSampleAgeMs = 10000, maxFutureSkewMs = 5000 } = options;
    if (!sample || typeof sample !== 'object' || Array.isArray(sample)) {
        return { allowed: false, reasons: [reason('MISSING_MEASUREMENT', 'A current memory measurement is required.')] };
    }
    if (sample.error) {
        return { allowed: false, reasons: [reason('MEASUREMENT_ERROR', 'The memory sampler reported a measurement failure.')] };
    }
    if (![nowMs, maxSampleAgeMs, maxFutureSkewMs].every(Number.isFinite) || maxSampleAgeMs < 0 || maxFutureSkewMs < 0) {
        return { allowed: false, reasons: [reason('INVALID_EVALUATION_CLOCK', 'The measurement freshness limits are invalid.')] };
    }
    const fields = ['totalPhysicalBytes', 'availablePhysicalBytes', 'committedBytes', 'commitLimitBytes'];
    if (!fields.every((field) => Number.isSafeInteger(sample[field]) && sample[field] >= 0)
        || sample.totalPhysicalBytes === 0 || sample.commitLimitBytes === 0
        || sample.availablePhysicalBytes > sample.totalPhysicalBytes
        || !Array.isArray(sample.processes)) {
        return { allowed: false, reasons: [reason('INVALID_MEASUREMENT', 'The memory measurement has missing or invalid counters.')] };
    }
    for (const process of sample.processes) {
        if (!process || !Number.isSafeInteger(process.pid) || process.pid <= 0
            || !Number.isSafeInteger(process.parentPid) || process.parentPid < 0
            || typeof process.name !== 'string' || !process.name
            || !Number.isSafeInteger(process.privateBytes) || process.privateBytes < 0
            || !Number.isSafeInteger(process.workingSetBytes) || process.workingSetBytes < 0) {
            return { allowed: false, reasons: [reason('INVALID_MEASUREMENT', 'The memory measurement has invalid process counters.')] };
        }
    }
    const timestampMs = typeof sample.timestamp === 'string' ? Date.parse(sample.timestamp) : NaN;
    if (!Number.isFinite(timestampMs)) {
        return { allowed: false, reasons: [reason('INVALID_MEASUREMENT', 'The memory measurement timestamp is invalid.')] };
    }
    const ageMs = nowMs - timestampMs;
    if (ageMs > maxSampleAgeMs) {
        return { allowed: false, reasons: [reason('STALE_MEASUREMENT', 'The memory measurement is no longer current.', ageMs, maxSampleAgeMs)] };
    }
    if (ageMs < -maxFutureSkewMs) {
        return { allowed: false, reasons: [reason('FUTURE_MEASUREMENT', 'The memory measurement timestamp is in the future.', -ageMs, maxFutureSkewMs)] };
    }
    return {
        allowed: true,
        reasons: [],
        metrics: {
            availablePhysicalBytes: sample.availablePhysicalBytes,
            committedBytes: sample.committedBytes,
            commitLimitBytes: sample.commitLimitBytes,
            commitHeadroomBytes: sample.commitLimitBytes - sample.committedBytes,
            commitPercent: 100 * sample.committedBytes / sample.commitLimitBytes,
            ageMs
        }
    };
}

function evaluateLimits(sample, policy, options, emergency) {
    const validation = validateSample(sample, options);
    if (!validation.allowed) return validation;
    const { metrics } = validation;
    const reasons = [];
    if (metrics.availablePhysicalBytes < policy.minAvailableBytes) {
        reasons.push(reason('LOW_PHYSICAL_MEMORY', 'Available physical memory is below the required reserve.', metrics.availablePhysicalBytes, policy.minAvailableBytes));
    }
    if (metrics.commitHeadroomBytes < policy.minCommitHeadroomBytes) {
        reasons.push(reason('LOW_COMMIT_HEADROOM', 'System commit headroom is below the required reserve.', metrics.commitHeadroomBytes, policy.minCommitHeadroomBytes));
    }
    if (emergency ? metrics.commitPercent >= policy.maxCommitPercent : metrics.commitPercent > policy.maxCommitPercent) {
        reasons.push(reason('HIGH_COMMIT_PERCENT', 'System commit usage has reached the workload limit.', metrics.commitPercent, policy.maxCommitPercent));
    }
    return { allowed: reasons.length === 0, reasons, metrics };
}

export function evaluatePreflight(sample, workload, options) {
    if (!Object.hasOwn(WORKLOAD_POLICIES, workload)) {
        return { allowed: false, reasons: [reason('UNKNOWN_WORKLOAD', 'The workload has no defined memory policy.')] };
    }
    return evaluateLimits(sample, WORKLOAD_POLICIES[workload], options, false);
}

export function evaluateEmergency(sample, options) {
    return evaluateLimits(sample, EMERGENCY_POLICY, options, true);
}
