// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validateFormatReport } from './direct-format-control.mjs';

test('private format controls admit only owned bounded one-shot phases', () => {
    const resourceId = 'owner';
    const report = { resourceId, phase: 'format-browser-00' };
    assert.equal(validateFormatReport(report, resourceId, true, new Set()), report.phase);
    assert.equal(validateFormatReport({ ...report, phase: 'format-native-13' }, resourceId, true, new Set()), 'format-native-13');
    for (const phase of ['format-native-14', 'format-browser-0', 'format-native-001', '../native', 'native', 'format-native-13\n']) {
        assert.throws(() => validateFormatReport({ ...report, phase }, resourceId, true, new Set()));
    }
    assert.throws(() => validateFormatReport(report, resourceId, false, new Set()));
    assert.throws(() => validateFormatReport(report, 'peer', true, new Set()));
    assert.throws(() => validateFormatReport(report, resourceId, true, new Set([report.phase])));
    assert.throws(() => validateFormatReport({ ...report, payload: 'x'.repeat(65536) }, resourceId, true, new Set()));
});
