// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function createLease({ remaining, renew, onUncertain, now = () => performance.now() }) {
    let timer = null;
    let inFlight = null;
    let allowed = false;
    let last = 0;
    let epoch = 0;
    function stop() { ++epoch; allowed = false; if (timer !== null) clearInterval(timer); timer = null; }
    function tick() {
        if (!allowed) return;
        try {
            if (remaining() <= 0 || now() - last > 60_000) { stop(); onUncertain('expired'); return; }
        } catch { stop(); onUncertain('expired'); return; }
        if (inFlight || now() - last < 15_000) return;
        last = now();
        const token = epoch;
        inFlight = Promise.resolve().then(renew).catch(() => { if (token === epoch) { stop(); onUncertain('uncertain'); } }).finally(() => { inFlight = null; });
    }
    function start() { stop(); allowed = true; last = now(); timer = setInterval(tick, 1000); }
    return { start, stop, tick, barrier: () => { ++epoch; }, drain: () => inFlight || Promise.resolve() };
}
