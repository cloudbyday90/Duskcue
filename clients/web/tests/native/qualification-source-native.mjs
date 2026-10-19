// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { runSourceMatrix } from './qualification-source.mjs';
import { sourceQuality, withNativeQualificationQuality } from './qualification-source-quality.mjs';

export async function runSourceNative({ page, ready, origin, directory, verifySource, abortSignal }) {
    const applicationOrigin = new URL(page.url()).origin;
    return withNativeQualificationQuality(page, sourceQuality, () => runSourceMatrix({ page, ready, origin, client: 'native', directory, verifySource, abortSignal,
        async openCase(candidate) { await page.goto(`${applicationOrigin}/play/${candidate.item}?file=${candidate.file}`); },
        async closeCase(candidate) {
            const close = page.getByRole('button', { name: 'Close player', exact: true });
            if (await close.count() === 1) await close.click();
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/media/${candidate.item}`);
        } }));
}
