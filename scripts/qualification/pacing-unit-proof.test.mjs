// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { PACING_UNIT_CASES, validatePacingUnitProof } from './pacing-unit-proof.mjs';

test('pacing unit evidence requires each named regression executed once without skips', () => {
    const rows = PACING_UNIT_CASES.map((name) => `test services::transcoding::qualification::tests::${name} ... ok`);
    const summary = 'test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 910 filtered out;';
    assert.equal(validatePacingUnitProof([...rows, summary].join('\n')).cases, 8);
    for (const lines of [[...rows.slice(1), summary], [...rows, rows[0], summary], [...rows, summary.replace('8 passed', '0 passed')], [...rows, summary.replace('0 ignored', '1 ignored')]]) assert.throws(() => validatePacingUnitProof(lines.join('\n')));
});
