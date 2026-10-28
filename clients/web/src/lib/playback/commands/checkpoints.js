// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { PROTOCOL, uuid, valid } from './schema.js';

export function createCheckpoints({ read, send, current, onUncertain, now = () => performance.now() }) {
    let sequence = 0;
    let inFlight = null;
    let latest = null;
    let uncertain = false;
    let last = -Infinity;
    let confirmed = null;
    let forced = null;
    let epoch = 0;
    function observe(value) {
        if (!value || !value.attached || !uuid(value.output) || !Number.isSafeInteger(value.position) || value.position < 0
            || !['playing', 'paused', 'buffering'].includes(value.state) || !current(value.output)) return;
        const previous = latest || confirmed && { output: confirmed.output_revision, state: confirmed.playback_state };
        latest = { ...value };
        return !previous || previous.output !== value.output || previous.state !== value.state;
    }
    async function flush(force = false) {
        if (inFlight) {
            if (!force) return inFlight;
            if (!forced) {
                const token = epoch;
                const work = inFlight.then(() => token === epoch ? flush(true) : null);
                forced = work;
                void work.finally(() => { if (forced === work) forced = null; }).catch(() => {});
            }
            return forced;
        }
        if (uncertain || !latest || !current(latest.output) || !force && now() - last < 15_000) return confirmed;
        const sample = latest;
        latest = null;
        const body = { protocol: PROTOCOL, output_revision: sample.output, sequence: ++sequence, position_ms: sample.position, playback_state: sample.state };
        const token = epoch;
        valid(Number.isSafeInteger(sequence));
        last = now();
        inFlight = Promise.resolve().then(() => send(body)).then((value) => {
            if (token !== epoch) return null;
            valid(value?.protocol === PROTOCOL && value.sequence === body.sequence && value.output_revision === body.output_revision
                && value.position_ms === body.position_ms && value.playback_state === body.playback_state);
            confirmed = value;
            return value;
        }).catch((error) => { if (token === epoch) { uncertain = true; onUncertain(); } throw error; }).finally(() => { inFlight = null; });
        return inFlight;
    }
    async function reconcile() {
        const token = epoch;
        if (inFlight) await inFlight.catch(() => {});
        if (token !== epoch) return null;
        const value = await read();
        if (token !== epoch) return null;
        valid(value?.protocol === PROTOCOL && Number.isSafeInteger(value.sequence) && value.sequence >= (confirmed?.sequence || 0)
            && (value.output_revision === null || uuid(value.output_revision) && current(value.output_revision)));
        sequence = Math.max(sequence, value.sequence);
        confirmed = value;
        uncertain = false;
        return value;
    }
    return { observe, flush, reconcile, needsReconcile: () => uncertain, confirmed: () => confirmed, discard: () => { latest = null; },
        drain: () => inFlight || Promise.resolve(),
        reset: () => { ++epoch; sequence = 0; confirmed = null; latest = null; uncertain = false; forced = null; } };
}
