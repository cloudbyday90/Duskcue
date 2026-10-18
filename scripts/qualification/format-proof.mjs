// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';

export function validateFormatUnitProof(output) {
    for (const name of ['inventory::tests::inventory_rejects_paths_and_unsupported_shapes', 'media::tests::avc_configuration_uses_actual_profile_compatibility_and_level', 'media::tests::frame_identifiers_distinguish_seek_target_from_start_and_damage']) {
        assert.equal(output.split(/\r?\n/).filter((line) => line === `test playback_progressive::direct::browser::formats::${name} ... ok`).length, 1);
    }
    assert.match(output, /^test result: ok\. 3 passed; 0 failed; 0 ignored;/m);
    return { passed: true, cases: 3, execution: 'Linux integration artifact; exact format verifier unit cases' };
}

export function validateFormatProcessProof(output) {
    for (const name of ['format process rejects excess streamed output before unbounded capture', 'format process deadline waits for child exit and does not leave its worker alive', 'format process rejects missing executable and invalid bounds', 'format file identity checks the byte ceiling and exact content', 'Linux format file identity refuses a symlink to an otherwise valid owned file']) {
        assert.equal(output.split(/\r?\n/).filter((line) => /^ok \d+ - /.test(line) && line.endsWith(` - ${name}`)).length, 1);
    }
    assert.match(output, /^# tests 5$/m);
    assert.match(output, /^# pass 5$/m);
    assert.match(output, /^# fail 0$/m);
    assert.match(output, /^# skipped 0$/m);
    return { passed: true, cases: 5, execution: 'Linux runtime image; bounded process and no-follow file identity helpers' };
}

export function validateFormatSummaries(browser, manifest) {
    assert.equal(manifest.version, 1);
    assert.ok(Array.isArray(manifest.cases) && manifest.cases.length === 14, 'Fourteen actual format sources are required.');
    const sourceIds = manifest.cases.map((item) => item.id);
    assert.equal(new Set(sourceIds).size, 14);
    for (const item of manifest.cases) {
        assert.match(item.id, /^[a-z0-9-]{1,80}$/);
        assert.match(item.sha256, /^[a-f0-9]{64}$/);
        assert.ok(['hevc', 'av1'].includes(item.codec));
        assert.ok([8, 10].includes(item.bitDepth));
        assert.ok(['sdr', 'hdr10', 'hlg'].includes(item.dynamicRange));
        assert.equal(item.videoIndex, 0);
        assert.equal(item.audioIndex, 2);
        assert.equal(item.audioHz, 880);
    }
    const masters = new Set(manifest.cases.map((item) => item.master));
    assert.equal(masters.size, 6);
    const specifications = [];
    for (const master of masters) {
        const variants = manifest.cases.filter((item) => item.master === master);
        const first = variants[0];
        const specification = `${first.codec}/${first.bitDepth}/${first.dynamicRange}`;
        specifications.push(specification);
        assert.deepEqual(variants.map((item) => item.container).sort(), first.codec === 'av1' ? ['mkv', 'mp4', 'webm'] : ['mkv', 'mp4']);
        assert.ok(variants.every((item) => `${item.codec}/${item.bitDepth}/${item.dynamicRange}` === specification));
    }
    assert.deepEqual(specifications.sort(), ['av1/10/sdr', 'av1/8/sdr', 'hevc/10/hdr10', 'hevc/10/hlg', 'hevc/10/sdr', 'hevc/8/sdr']);
    for (const [summary, client] of [[browser.formats, 'browser'], [browser.native?.checks?.formats, 'native']]) {
        assert.deepEqual(summary, { passed: true, cases: 14, played: 10, refused: 4, client, sourceIds });
    }
    return { passed: true, cases: 14, clients: 2, played: 20, refused: 8, sourceIds };
}
