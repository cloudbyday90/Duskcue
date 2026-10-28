// Duskcue â€” Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { installSourceRecoveryErrors } from './qualification-source-recovery-observation.mjs';
import { pauseThroughUi } from './qualification-pause.mjs';

export async function awaitVodAttachment(video, previous = null) {
    await expect.poll(() => video.evaluate((element, source) => !!element.currentSrc && element.currentSrc !== source, previous), { timeout: 15000 }).toBe(true);
}

export async function exerciseVodDecoder({ page, region, video, recoveries, selectAudio = false, onStage = (_name) => {} }) {
    onStage('admission-observation');
    await awaitVodAttachment(video);
    onStage('initial-copy-startup');
    await expect.poll(() => video.evaluate((element) => element.readyState >= 2 && element.videoWidth > 0), { timeout: 5000 }).toBe(true);
    await pauseThroughUi(region, video);
    await page.evaluate(installSourceRecoveryErrors);
    if (selectAudio) {
        const previous = await video.evaluate((element) => element.currentSrc);
        await region.getByRole('button', { name: 'Audio & subtitles', exact: true }).click();
        const choices = region.getByRole('region', { name: 'Audio & subtitles', exact: true });
        const french = choices.getByRole('button', { name: 'French', exact: true });
        const alternate = await french.getAttribute('aria-pressed') === 'true' ? choices.getByRole('button', { name: 'English', exact: true }) : french;
        await alternate.click();
        await awaitVodAttachment(video, previous);
        await expect.poll(() => video.evaluate((element) => element.readyState >= 2), { timeout: 5000 }).toBe(true);
    }
    const failedSource = await video.evaluate((element) => element.currentSrc);
    onStage('healthy-copy-armed');
    await page.evaluate(() => window.sourceRecoveryErrors.arm('protocol-two-copy'));
    await region.getByRole('button', { name: 'Play', exact: true }).click();
    await expect.poll(() => recoveries().length, { timeout: 75000 }).toBeGreaterThan(0);
    const errors = await page.evaluate(() => window.sourceRecoveryErrors.snapshot());
    onStage('genuine-decoder-error');
    expect(errors.some((error) => error.trusted && error.code === 3 && error.source === failedSource)).toBe(true);
    await awaitVodAttachment(video, failedSource);
    await expect.poll(() => video.evaluate((element, source) => element.currentSrc !== source && element.readyState >= 2
        && element.currentTime > .1 && (element.getVideoPlaybackQuality?.().totalVideoFrames || 0) > 0, failedSource), { timeout: 5000 }).toBe(true);
    await expect(region.locator('.recovery-status')).toBeVisible();
    onStage('replacement-decoded');
    await pauseThroughUi(region, video);
    expect(recoveries().every((record) => record.evidenceKind === 'hls_media' && record.status === 202)).toBe(true);
    await page.evaluate(() => window.sourceRecoveryErrors.detach());
    return { genuineTrustedDecoderError: true, retainedRecoverCommands: recoveries().length, replacementDecoded: true };
}
