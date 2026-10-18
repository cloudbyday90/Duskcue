// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { FRAME_ID, HEIGHT, MASTERS, WIDTH, hlgOetf, narrowCode, pqEotf, pqInverseEotf, referenceFrame, referencePatches } from './format-reference.mjs';
import { audioEncodingArguments, encodingArguments } from './format-corpus.mjs';

test('BT2100 PQ independently fixed reference points retain absolute luminance and narrow codes', () => {
    assert.ok(Math.abs(pqInverseEotf(100) - 0.508078421517399) < 1e-12);
    assert.ok(Math.abs(pqInverseEotf(1000) - 0.751827096247041) < 1e-12);
    assert.deepEqual([0, 0.1, 1, 10, 100, 203, 400, 1000].map((nits) => narrowCode(pqInverseEotf(nits), 10)), [64, 119, 195, 327, 509, 573, 636, 723]);
    for (const nits of [0.1, 1, 10, 100, 203, 400, 1000]) assert.ok(Math.abs(pqEotf(pqInverseEotf(nits)) - nits) < 1e-7);
    for (const value of [-1, 10001, NaN, Infinity]) assert.throws(() => pqInverseEotf(value));
});

test('HLG scene OETF has the correct piecewise junction and differs numerically from PQ', () => {
    assert.equal(hlgOetf(0), 0);
    assert.equal(hlgOetf(1 / 12), 0.5);
    assert.ok(Math.abs(hlgOetf(1) - 1) < 1e-8);
    assert.deepEqual([0, 1 / 1024, 1 / 64, 1 / 12, 1].map((scene) => narrowCode(hlgOetf(scene), 10)), [64, 111, 254, 502, 940]);
    assert.notEqual(narrowCode(hlgOetf(1), 10), narrowCode(pqInverseEotf(1000), 10));
    for (const value of [-0.1, 1.1, NaN, Infinity]) assert.throws(() => hlgOetf(value));
});

test('authored frame IDs distinguish source five seconds and preserve native ten-bit neutral samples', () => {
    for (const master of MASTERS) {
        const first = referenceFrame(master, 0);
        const sought = referenceFrame(master, 120);
        const sample = (bytes, x, y) => master.bitDepth === 10 ? bytes.readUInt16LE((y * WIDTH + x) * 2) : bytes[y * WIDTH + x];
        assert.equal(first.length, WIDTH * HEIGHT * 3 / 2 * (master.bitDepth === 10 ? 2 : 1));
        const black = narrowCode(0, master.bitDepth);
        const white = master.dynamicRange === 'hdr10' ? 723 : narrowCode(1, master.bitDepth);
        for (let bit = 0; bit < FRAME_ID.bits; bit++) {
            assert.equal(sample(first, 24 + 32 * bit, 164), black);
            assert.equal(sample(sought, 24 + 32 * bit, 164), 120 & (1 << bit) ? white : black);
        }
        assert.notDeepEqual(first, sought);
        const patches = referencePatches(master);
        for (let patch = 0; patch < patches.length; patch++) assert.equal(sample(sought, 24 + 32 * patch, 24), patches[patch].code);
        if (master.bitDepth === 10) assert.ok(patches.some((patch) => patch.code % 4 !== 0));
    }
    assert.equal(MASTERS.filter((master) => master.dynamicRange === 'sdr' && master.bitDepth === 10).length, 2);
    assert.throws(() => referenceFrame(MASTERS[0], 192));
});

test('HEVC uses fixed ordinary-profile QP1 while AV1 alone requests raw-exact lossless encoding', () => {
    for (const master of MASTERS) {
        const args = encodingArguments(master, { pixelFormat: master.bitDepth === 10 ? 'yuv420p10le' : 'yuv420p', masteringDisplay: 'test-mastering', maxCll: 1005, maxFall: 200 });
        assert.equal(args[args.indexOf('-threads') + 1], '1');
        if (master.codec === 'hevc') {
            assert.equal(args[args.indexOf('-profile:v') + 1], master.bitDepth === 10 ? 'main10' : 'main');
            const options = args[args.indexOf('-x265-params') + 1].split(':');
            for (const option of ['qp=1', 'lossless=0', 'pools=none', 'frame-threads=1', 'bframes=0', 'keyint=48']) assert.ok(options.includes(option));
            assert.equal(options.some((option) => option.startsWith('master-display=')), master.dynamicRange === 'hdr10');
        } else {
            assert.equal(args[args.indexOf('-aom-params') + 1], 'lossless=1');
            assert.equal(args[args.indexOf('-lag-in-frames') + 1], '0');
        }
    }
});

test('reference Opus uses the aligned lowdelay120-sample packet configuration', () => {
    const args = audioEncodingArguments();
    for (const [flag, expected] of [['-c:a', 'libopus'], ['-application', 'lowdelay'], ['-frame_duration', '2.5'], ['-threads', '1'], ['-ar', '48000'], ['-ac', '1']]) assert.equal(args[args.indexOf(flag) + 1], expected);
    assert.equal(48000 * Number(args[args.indexOf('-frame_duration') + 1]) / 1000, 120);
    assert.equal((48000 * 8 + 120) % 120, 0);
});
