// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { codecConfiguration, mp4CodecConfiguration } from './format-codecs.mjs';

function hvcc(depth = 8) {
    const header = Buffer.alloc(23);
    header[0] = 1; header[1] = depth === 10 ? 2 : 1; header.writeUInt32BE(0x60000000, 2); header[6] = 0xb0; header[12] = 60;
    header[13] = 0xf0; header[15] = 0xfc; header[16] = 0xfd; header[17] = 0xf8 | (depth - 8); header[18] = header[17]; header[21] = 3; header[22] = 3;
    return Buffer.concat([header, ...[32, 33, 34].map((kind) => Buffer.from([0x80 | kind, 0, 1, 0, 2, kind << 1, 1]))]);
}

function box(kind, ...contents) {
    const payload = Buffer.concat(contents);
    const header = Buffer.alloc(8);
    header.writeUInt32BE(payload.length + 8); header.write(kind, 4, 'ascii');
    return Buffer.concat([header, payload]);
}

function mp4(configuration, sample = 'hvc1', kind = 'hvcC') {
    const stsd = Buffer.alloc(8); stsd.writeUInt32BE(1, 4);
    return box('moov', box('trak', box('mdia', box('minf', box('stbl', box('stsd', stsd, box(sample, Buffer.alloc(78), box(kind, configuration))))))));
}

test('HEVC codec descriptors retain actual profile compatibility, tier, level, constraints and ten-bit configuration', () => {
    assert.equal(codecConfiguration('hevc', hvcc()).codecString, 'hvc1.1.6.L60.B0');
    const main10 = codecConfiguration('hevc', hvcc(10));
    assert.equal(main10.codecString, 'hvc1.2.6.L60.B0');
    assert.equal(main10.bitDepth, 10);
    assert.equal(main10.configurationHex, hvcc(10).toString('hex'));
    assert.deepEqual(mp4CodecConfiguration(mp4(hvcc(10))), main10);
    for (const length of [0, 12, 22, 25, hvcc().length - 1]) assert.throws(() => codecConfiguration('hevc', hvcc().subarray(0, length)));
    const rext = hvcc(); rext[1] = 4;
    assert.throws(() => codecConfiguration('hevc', rext), /profile/);
    const invalidReserved = hvcc(); invalidReserved[17] = 0;
    assert.throws(() => codecConfiguration('hevc', invalidReserved));
});

test('AV1 level index zero is valid and actual high-bitdepth changes the codec string', () => {
    const eight = Buffer.from([0x81, 0, 0x0c, 0, 0x0a, 1, 0]);
    const ten = Buffer.from(eight); ten[2] |= 64;
    assert.equal(codecConfiguration('av1', eight).codecString, 'av01.0.00M.08');
    assert.equal(codecConfiguration('av1', ten).codecString, 'av01.0.00M.10');
    assert.equal(mp4CodecConfiguration(mp4(ten, 'av01', 'av1C')).bitDepth, 10);
    for (const index of [0, 1, 2, 3, 4, 5]) {
        const broken = Buffer.from(eight);
        broken[index] = index === 2 ? 0x8c : 0xff;
        assert.throws(() => codecConfiguration('av1', broken));
    }
    assert.throws(() => codecConfiguration('av1', Buffer.concat([eight, eight.subarray(4)])), /sequence header/);
    assert.throws(() => codecConfiguration('av1', eight.subarray(0, 6)), /Truncated/);
});

test('bounded MP4 extraction rejects fake markers, ambiguous entries and malformed box lengths', () => {
    assert.throws(() => mp4CodecConfiguration(box('mdat', Buffer.from('hvcC'), hvcc())), /one actual/);
    const valid = mp4(hvcc());
    assert.throws(() => mp4CodecConfiguration(Buffer.concat([valid, valid])), /one actual/);
    assert.throws(() => mp4CodecConfiguration(valid.subarray(0, valid.length - 1)), /Truncated/);
    const huge = Buffer.from(valid); huge.writeUInt32BE(0xffffffff);
    assert.throws(() => mp4CodecConfiguration(huge), /Truncated/);
    assert.throws(() => mp4CodecConfiguration(Buffer.alloc(8388609)), /bound/);
});
