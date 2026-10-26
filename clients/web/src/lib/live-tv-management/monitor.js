/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */
export function createRefreshMonitor({ read, visible = () => globalThis.document?.visibilityState !== 'hidden', now = () => performance.now(), schedule = setTimeout, cancel = clearTimeout }) {
    let generation = 0; let timer = null; let deadline = 0;
    function stop() { generation += 1; cancel(timer); timer = null; }
    return {
        stop,
        start() {
            stop(); deadline = now() + 300000; const run = generation;
            const next = async () => {
                if (run !== generation || !visible() || now() >= deadline) return;
                try {
                    const status = await read();
                    if (run !== generation || !visible() || now() >= deadline || !status || status.observation !== 'current' || !status.locally_owned || status.latest?.state !== 'running') return;
                    timer = schedule(next, 5000);
                } catch { stop(); }
            };
            return next();
        },
    };
}
