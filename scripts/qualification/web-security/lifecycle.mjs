// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export async function qualifyBoundaries(operations, signal) {
    signal.throwIfAborted();
    const inputs = await operations.prerequisite(signal);
    signal.throwIfAborted();
    let result;
    let failure;
    try {
        await operations.prepare(inputs, signal);
        signal.throwIfAborted();
        await operations.build(inputs, signal);
        signal.throwIfAborted();
        result = await operations.run(inputs, signal);
        signal.throwIfAborted();
        if (!result || result.reroute !== true || result.staticPatterns !== true || result.cacheAttribute !== true) {
            throw new Error('Executed boundary evidence is incomplete.');
        }
    } catch (error) { failure = error; }
    try { await operations.dispose(); }
    catch (error) { failure = failure ? new AggregateError([failure, error], 'Boundary execution and cleanup failed.') : error; }
    if (signal.aborted && !failure) failure = signal.reason || new Error('Qualification cancelled during cleanup.');
    if (failure) throw failure;
    return { ...result, cleanupConfirmed: true, completeSecurityMatrix: false };
}

export function observe(promise, timeoutMs, message) {
    let timer;
    return Promise.race([
        promise,
        new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(message)), timeoutMs); }),
    ]).finally(() => clearTimeout(timer));
}
