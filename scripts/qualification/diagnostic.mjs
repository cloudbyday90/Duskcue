// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { runBounded } from './commands.mjs';
import { PRODUCER_CASE, EVENT_CANDIDATE_CASE } from './cases.mjs';
import { RUNTIME_ARTIFACTS } from './attest.mjs';

export const TRACE_LOG = '/qualification/work/producer-strace.log';
export const TRACE_LIMITS = Object.freeze({ bytes: 16777216, timeoutMs: 90000, captureBytes: 1048576 });

export function traceSpecification(selector) {
    if (selector === PRODUCER_CASE) return { name: 'producer', trace: TRACE_LOG, sourceRequired: false };
    if (selector === EVENT_CANDIDATE_CASE) return { name: 'event', trace: '/qualification/work/event-strace.log', sourceRequired: true };
    throw new Error('Only the exact current producer or EVENT candidate may be traced.');
}

export function traceCommand(executable, selector) {
    if (executable !== RUNTIME_ARTIFACTS.libTest) throw new Error('Only the current qualified library executable may be traced.');
    traceSpecification(selector);
    return { executable: '/usr/bin/strace', args: ['-f', '-qq', '-s', '0', '-e', 'raw=all', '--', executable, selector, '--exact', '--include-ignored', '--nocapture'] };
}

export function traceSummary(result, selector = PRODUCER_CASE) {
    return {
        completed: true,
        countsTowardQualification: false,
        exitCode: result.code,
        signal: result.signal,
        observedSigsys: /(?:killed by SIGSYS|SIGSYS \{[^\n]*si_code=SYS_SECCOMP)/.test(result.stderr),
        tracingRefused: /(?:ptrace|PTRACE)[^\n]*(?:Operation not permitted|Permission denied)/i.test(result.stderr),
        bytes: result.bytes,
        trace: traceSpecification(selector).trace,
    };
}

export async function traceQualifiedCase(executable, selector) {
    const command = traceCommand(executable, selector);
    const { trace } = traceSpecification(selector);
    try {
        const result = await runBounded(command.executable, command.args, { ...TRACE_LIMITS, log: trace });
        return traceSummary(result, selector);
    } catch (error) {
        return { completed: false, countsTowardQualification: false, error: error.message, trace };
    }
}
