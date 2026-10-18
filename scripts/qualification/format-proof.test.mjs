// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { validateFormatProcessProof, validateFormatSummaries, validateFormatUnitProof } from './format-proof.mjs';

test('format compiler and process gates require exact executed cases with no skips', () => {
    const unit = ['inventory::tests::inventory_rejects_paths_and_unsupported_shapes', 'media::tests::avc_configuration_uses_actual_profile_compatibility_and_level', 'media::tests::frame_identifiers_distinguish_seek_target_from_start_and_damage']
        .map((name) => `test playback_progressive::direct::browser::formats::${name} ... ok`).join('\n') + '\ntest result: ok. 3 passed; 0 failed; 0 ignored;\n';
    assert.equal(validateFormatUnitProof(unit).cases, 3);
    assert.throws(() => validateFormatUnitProof(unit.replace('... ok', '... ignored')));
    const process = ['format process rejects excess streamed output before unbounded capture', 'format process deadline waits for child exit and does not leave its worker alive', 'format process rejects missing executable and invalid bounds', 'format file identity checks the byte ceiling and exact content', 'Linux format file identity refuses a symlink to an otherwise valid owned file']
        .map((name, index) => `ok ${index + 1} - ${name}`).join('\n') + '\n# tests 5\n# pass 5\n# fail 0\n# skipped 0\n';
    assert.equal(validateFormatProcessProof(process).cases, 5);
    assert.throws(() => validateFormatProcessProof(process.replace('# skipped 0', '# skipped 1')));
    assert.throws(() => validateFormatProcessProof(process.replace('ok 5', 'not ok 5')));
});

function evidence() {
    const specifications = [['hevc', 8, 'sdr'], ['hevc', 10, 'sdr'], ['av1', 8, 'sdr'], ['av1', 10, 'sdr'], ['hevc', 10, 'hdr10'], ['hevc', 10, 'hlg']];
    const cases = specifications.flatMap(([codec, bitDepth, dynamicRange], master) => (codec === 'av1' ? ['mp4', 'mkv', 'webm'] : ['mp4', 'mkv']).map((container) => ({
        id: `source-${master}-${container}`, master: String(master), container, codec, bitDepth, dynamicRange, sha256: 'a'.repeat(64), videoIndex: 0, audioIndex: 2, audioHz: 880,
    })));
    const summary = { passed: true, cases: 14, played: 10, refused: 4, sourceIds: cases.map((item) => item.id) };
    return { manifest: { version: 1, cases }, browser: { formats: { ...summary, client: 'browser' }, native: { checks: { formats: { ...summary, client: 'native' } } } } };
}

test('format evidence cannot substitute a missing HDR/precision/container case or one client', () => {
    const valid = evidence();
    assert.equal(validateFormatSummaries(valid.browser, valid.manifest).played, 20);
    for (const mutate of [
        ({ manifest }) => { manifest.cases[0].bitDepth = 10; },
        ({ manifest }) => { for (const item of manifest.cases) if (item.dynamicRange === 'hlg') item.dynamicRange = 'hdr10'; },
        ({ manifest }) => { manifest.cases[0].container = 'webm'; },
        ({ manifest }) => { manifest.cases[0].sha256 = ''; },
        ({ manifest }) => { manifest.cases[0].audioIndex = 1; },
        ({ browser }) => { browser.native.checks.formats.sourceIds.reverse(); },
        ({ browser }) => { browser.formats.refused = 0; },
        ({ browser }) => { delete browser.native; },
    ]) {
        const changed = structuredClone(evidence());
        mutate(changed);
        assert.throws(() => validateFormatSummaries(changed.browser, changed.manifest));
    }
});
