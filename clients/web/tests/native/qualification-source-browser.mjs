// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { runSourceMatrix } from './qualification-source.mjs';
import { sourceQuality, withBrowserQualificationQuality } from './qualification-source-quality.mjs';

export async function runSourceBrowser({ page, ready, origin, directory, verifySource, abortSignal }) {
    return withBrowserQualificationQuality(page, ready, sourceQuality, () => runSourceMatrix({ page, ready, origin, client: 'browser', directory, verifySource, abortSignal,
        async openCase(candidate) {
            await page.evaluate(({ fixture, candidate }) => window.directFixture.mountFormat(fixture, candidate), {
                fixture: { profileId: ready.profileId, userId: ready.userId }, candidate,
            });
        },
        async closeCase() {
            const close = page.getByRole('button', { name: 'Close player', exact: true });
            if (await close.count() === 1) await close.click();
            await page.evaluate(() => window.directFixture.stop());
        } }));
}
