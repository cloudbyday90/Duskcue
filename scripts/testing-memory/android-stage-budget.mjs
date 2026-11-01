// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function androidStageBudget(suiteDeadline, { now = Date.now(), bytes = 16777216 } = {}) {
    if (!Number.isSafeInteger(suiteDeadline) || suiteDeadline <= now) throw new Error('Shared native suite deadline expired.');
    const deadline = Math.min(suiteDeadline, now + 240000);
    const bootstrapDeadline = Math.min(deadline, now + 30000);
    const cancellation = new AbortController();
    let total = 0, failure;
    const fail = (error) => { failure ||= error; cancellation.abort(); };
    const timer = setTimeout(() => fail(new Error('Android stage exceeded its deadline.')), deadline - now);
    return {
        deadline, bootstrapDeadline, signal: cancellation.signal,
        get bytes() { return total; }, get failure() { return failure; },
        fail,
        consume(size) {
            if (!Number.isSafeInteger(size) || size < 0) throw new Error('Invalid Android output accounting.');
            total += size;
            if (total > bytes) fail(new Error('Android stage exceeded its shared output budget.'));
            if (failure) throw failure;
        },
        remaining(bootstrap = false) {
            if (failure) throw failure;
            const remaining = (bootstrap ? bootstrapDeadline : deadline) - Date.now();
            if (remaining <= 0) { fail(new Error('Android bootstrap or stage deadline expired.')); throw failure; }
            return remaining;
        },
        close() { clearTimeout(timer); },
    };
}

export function androidLogFrames(onLine, maxBytes = 65536) {
    let pending = Buffer.alloc(0);
    const decoder = new TextDecoder('utf-8', { fatal: true });
    return {
        write(chunk) {
            pending = Buffer.concat([pending, chunk]);
            while (true) {
                const index = pending.indexOf(10);
                if (index < 0) break;
                if (index > maxBytes) throw new Error('Android log frame exceeded its bound.');
                onLine(decoder.decode(pending.subarray(0, index)).replace(/\r$/, ''));
                pending = pending.subarray(index + 1);
            }
            if (pending.length > maxBytes) throw new Error('Android log frame exceeded its bound.');
        },
        finish() { if (pending.length) throw new Error('Android log ended with an incomplete frame.'); },
    };
}
