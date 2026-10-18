// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { createFormatPacing, waitForFormatPacing } from './qualification-format-pacing.mjs';

test('format pacing refills first and preserves measured twenty/ twenty-eight-second case start intervals', async () => {
    for (const [client, interval] of [['browser', 20000], ['native', 28000]]) {
        let clock = 0;
        const waits = [];
        const pacing = createFormatPacing({ client, now: () => clock, wait: async (milliseconds) => { waits.push(milliseconds); clock += milliseconds; } });
        await assert.rejects(pacing.next());
        await pacing.begin();
        assert.equal(clock, 30000);
        assert.equal((await pacing.next()).intervalMs, null);
        clock += 4000;
        assert.deepEqual(await pacing.next(), { sinceMatrixStartMs: 30000 + interval, intervalMs: interval, waitedMs: interval - 4000 });
        clock += interval + 1000;
        assert.equal((await pacing.next()).waitedMs, 0);
        assert.deepEqual(waits, [30000, interval - 4000]);
        assert.equal(pacing.evidence.actualInitialRefillMs, 30000);
        await assert.rejects(pacing.begin());
    }
});

test('format pacing rejects invalid waits, absent clock progress and canceled admission', async () => {
    for (const milliseconds of [-1, 30001, Number.NaN, 0.5]) await assert.rejects(waitForFormatPacing(milliseconds));
    const stopped = new AbortController(); stopped.abort();
    await assert.rejects(waitForFormatPacing(30000, stopped.signal), /canceled/);
    const controller = new AbortController();
    const waiting = waitForFormatPacing(30000, controller.signal);
    controller.abort();
    await assert.rejects(waiting, /canceled/);
    await assert.rejects(createFormatPacing({ client: 'browser', now: () => 0, wait: async () => {} }).begin(), /clock did not advance/);
    let clock = 0;
    const admission = new AbortController();
    const pacing = createFormatPacing({ client: 'native', signal: admission.signal, now: () => clock,
        wait: async (milliseconds) => { clock += milliseconds; admission.abort(); } });
    await assert.rejects(pacing.begin(), /canceled/);
    assert.equal(pacing.evidence.starts.length, 0);
});
