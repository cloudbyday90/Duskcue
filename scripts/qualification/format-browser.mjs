// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { runFormatMatrix } from '../../clients/web/tests/native/qualification-format.mjs';
import { conservativeQuality, withBrowserQualificationQuality } from '../../clients/web/tests/native/qualification-source-quality.mjs';

export async function runFormatBrowser({ page, ready, origin, directory, verifyFormat, abortSignal }) {
    return withBrowserQualificationQuality(page, ready, conservativeQuality, () => runFormatMatrix({ page, ready, origin, client: 'browser', directory, verifyFormat, abortSignal,
        async openCase(candidate) {
            await page.evaluate(({ fixture, candidate }) => window.directFixture.mountFormat(fixture, candidate), {
                fixture: { profileId: ready.profileId, userId: ready.userId }, candidate,
            });
        },
        async closeCase() {
            const region = page.getByRole('region', { name: 'Media player', exact: true });
            const close = region.getByRole('button', { name: 'Close player', exact: true });
            if (await close.count() === 1) await close.click();
            await page.evaluate(() => window.directFixture.stop());
        } }));
}
