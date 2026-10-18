// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { evaluateLinuxHeadroom, readLinuxSample } from './linux-engine-headroom.mjs';
import { createLinuxTelemetry } from './linux-telemetry.mjs';

export async function watchLinuxReserve(engine, recordSample, readSample = readLinuxSample) {
    const baseline = await readSample(engine);
    const admission = evaluateLinuxHeadroom(baseline, baseline);
    if (!admission.allowed) throw new Error(admission.reason);
    await recordSample(baseline);
    const controller = new AbortController();
    const telemetry = createLinuxTelemetry(baseline);
    let failure;
    let polling;
    const fail = (reason) => { failure ||= reason; controller.abort(); };
    const accept = async (sample, preflight) => {
        const decision = telemetry.observe(sample, { preflight });
        await recordSample(sample);
        if (!decision.allowed) { fail(decision.reason); throw new Error(decision.reason); }
        return { sample, decision };
    };
    const timer = setInterval(() => {
        if (failure) return;
        const decision = telemetry.check();
        if (!decision.allowed) { fail(decision.reason); return; }
        if (polling) return;
        polling = readSample(engine).then((sample) => accept(sample, false)).catch((error) => {
            const current = telemetry.readFailed(error);
            if (!current.allowed) fail(current.reason);
        }).finally(() => { polling = null; });
    }, 2000);
    return {
        signal: controller.signal,
        initial: { sample: baseline, decision: admission },
        async admit() {
            if (failure) throw new Error(failure);
            return accept(await readSample(engine), true);
        },
        async finish() {
            clearInterval(timer);
            if (polling) await polling;
            if (!failure) {
                const current = telemetry.check();
                if (!current.allowed) fail(current.reason);
            }
            if (failure) throw new Error(failure);
        },
    };
}
