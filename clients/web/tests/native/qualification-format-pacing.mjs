// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

const canceled = () => new Error('Format pacing was canceled.');

export function waitForFormatPacing(milliseconds, signal) {
    if (!Number.isSafeInteger(milliseconds) || milliseconds < 0 || milliseconds > 30000) return Promise.reject(new Error('Format pacing exceeded its wait bound.'));
    if (signal?.aborted) return Promise.reject(canceled());
    return new Promise((resolve, reject) => {
        const abort = () => { clearTimeout(timer); signal?.removeEventListener('abort', abort); reject(canceled()); };
        const timer = setTimeout(() => { signal?.removeEventListener('abort', abort); resolve(); }, milliseconds);
        signal?.addEventListener('abort', abort, { once: true });
        if (signal?.aborted) abort();
    });
}

export function createFormatPacing({ client, signal, now = () => performance.now(), wait = waitForFormatPacing } = {}) {
    if (!['browser', 'native'].includes(client)) throw new Error('Format pacing requires its real client identity.');
    const minimumStartIntervalMs = client === 'native' ? 28000 : 20000;
    const matrixStart = now();
    let lastStart = null;
    let admitted = false;
    const evidence = { initialRefillMs: 30000, minimumStartIntervalMs, starts: [], maxObservedClientRequests: 0 };
    const hold = async (target) => {
        for (let attempt = 0; attempt < 3; attempt += 1) {
            if (signal?.aborted) throw canceled();
            const before = now();
            if (!Number.isFinite(before)) throw new Error('Format pacing lost its monotonic clock.');
            const remaining = target - before;
            if (remaining <= 0) return;
            await wait(Math.min(30000, Math.ceil(remaining)), signal);
            if (now() <= before) throw new Error('Format pacing clock did not advance.');
        }
        if (now() < target) throw new Error('The minimum format pacing interval was not observed.');
    };
    return {
        evidence,
        async begin() {
            if (admitted) throw new Error('Format pacing admission cannot be repeated.');
            await hold(matrixStart + evidence.initialRefillMs);
            evidence.actualInitialRefillMs = now() - matrixStart;
            admitted = true;
        },
        async next() {
            if (!admitted) throw new Error('Format pacing requires its initial refill.');
            const before = now();
            if (lastStart !== null) await hold(lastStart + minimumStartIntervalMs);
            if (signal?.aborted) throw canceled();
            const started = now();
            const record = { sinceMatrixStartMs: started - matrixStart, intervalMs: lastStart === null ? null : started - lastStart, waitedMs: started - before };
            evidence.starts.push(record);
            lastStart = started;
            return record;
        },
    };
}
