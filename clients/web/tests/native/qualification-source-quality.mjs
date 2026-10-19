// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';

export const conservativeQuality = Object.freeze({ quality_mode: 'manual', max_streaming_bitrate: 1500000 });
export const sourceQuality = Object.freeze({ quality_mode: 'auto', max_streaming_bitrate: null });

export async function setNativeQualificationQuality(page, quality) {
    const origin = new URL(page.url()).origin;
    await page.goto(`${origin}/settings/preferences`);
    const select = page.locator('#pref-quality');
    await expect(select).toBeEnabled();
    const previous = { quality_mode: await select.inputValue(), max_streaming_bitrate: null };
    if (previous.quality_mode === 'manual') previous.max_streaming_bitrate = Number(await page.locator('#pref-bitrate').inputValue());
    await select.selectOption(quality.quality_mode);
    if (quality.quality_mode === 'manual') await page.locator('#pref-bitrate').selectOption(String(quality.max_streaming_bitrate));
    const save = page.getByRole('button', { name: 'Save changes', exact: true });
    if (await save.isEnabled()) {
        await save.click();
        await expect(page.getByText('Changes saved.', { exact: true })).toBeVisible();
    }
    await expect(select).toHaveValue(quality.quality_mode);
    if (quality.quality_mode === 'manual') await expect(page.locator('#pref-bitrate')).toHaveValue(String(quality.max_streaming_bitrate));
    return previous;
}

export async function withNativeQualificationQuality(page, quality, run) {
    const previous = await setNativeQualificationQuality(page, quality);
    try { return await run(); }
    finally { await setNativeQualificationQuality(page, previous); }
}

export async function withBrowserQualificationQuality(page, ready, quality, run) {
    const fixture = { profileId: ready.profileId, userId: ready.userId };
    const previous = await page.evaluate(({ fixture, quality }) => window.directFixture.setQuality(fixture, quality), { fixture, quality });
    try { return await run(); }
    finally { await page.evaluate(({ fixture, previous }) => window.directFixture.setQuality(fixture, previous), { fixture, previous }); }
}
