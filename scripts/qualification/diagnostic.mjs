// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { runBounded } from './commands.mjs';
import { PRODUCER_CASE } from './cases.mjs';
import { RUNTIME_ARTIFACTS } from './attest.mjs';

export const TRACE_LOG = '/qualification/work/producer-strace.log';
export const TRACE_LIMITS = Object.freeze({ bytes: 16777216, timeoutMs: 90000, captureBytes: 1048576 });

export function traceCommand(executable, selector) {
    if (executable !== RUNTIME_ARTIFACTS.libTest || selector !== PRODUCER_CASE) throw new Error('Only the exact current producer may be traced.');
    return { executable: '/usr/bin/strace', args: ['-f', '-qq', '-s', '0', '-e', 'raw=all', '--', executable, selector, '--exact', '--include-ignored', '--nocapture'] };
}

export function traceSummary(result) {
    return {
        completed: true,
        countsTowardQualification: false,
        exitCode: result.code,
        signal: result.signal,
        observedSigsys: /(?:killed by SIGSYS|SIGSYS \{[^\n]*si_code=SYS_SECCOMP)/.test(result.stderr),
        tracingRefused: /(?:ptrace|PTRACE)[^\n]*(?:Operation not permitted|Permission denied)/i.test(result.stderr),
        bytes: result.bytes,
        trace: TRACE_LOG,
    };
}

export async function traceProducer(executable, selector) {
    const command = traceCommand(executable, selector);
    try {
        const result = await runBounded(command.executable, command.args, { ...TRACE_LIMITS, log: TRACE_LOG });
        return traceSummary(result);
    } catch (error) {
        return { completed: false, countsTowardQualification: false, error: error.message, trace: TRACE_LOG };
    }
}
