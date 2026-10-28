// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { assertFormatPaused, formatPauseReady } from './qualification-format-policy.mjs';

export async function pausedThroughUi(region, video) {
    await expect.poll(async () => {
        const state = await video.evaluate((element) => ({ ready: element.readyState, paused: element.paused, ended: element.ended }));
        state.playVisible = await region.getByRole('button', { name: 'Play', exact: true }).isVisible();
        state.pauseVisible = await region.getByRole('button', { name: 'Pause', exact: true }).isVisible();
        try { assertFormatPaused(state); return true; } catch { return false; }
    }).toBe(true);
}

export async function pauseThroughUi(region, video, replayEnded = false) {
    await expect.poll(async () => {
        const state = await video.evaluate((element) => ({ ready: element.readyState, paused: element.paused, ended: element.ended }));
        state.playVisible = await region.getByRole('button', { name: 'Play', exact: true }).isVisible();
        state.pauseVisible = await region.getByRole('button', { name: 'Pause', exact: true }).isVisible();
        return formatPauseReady(state, replayEnded);
    }).toBe(true);
    await region.hover();
    const replayedEnded = await video.evaluate((element) => element.ended);
    if (replayedEnded && !replayEnded) throw new Error('Measured format playback ended before the trusted Pause.');
    if (await video.evaluate((element) => element.paused)) await region.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(() => video.evaluate((element) => element.readyState >= 2 && !element.paused && !element.ended)).toBe(true);
    await region.getByRole('button', { name: 'Pause', exact: true }).click();
    await pausedThroughUi(region, video);
    return { replayedEnded };
}
