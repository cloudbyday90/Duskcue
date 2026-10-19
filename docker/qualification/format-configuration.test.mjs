// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { demuxedConfiguration } from './format-configuration.mjs';

function hvcc(complete) {
    const header = Buffer.alloc(23);
    header[0] = 1; header[1] = 1; header.writeUInt32BE(0x60000000, 2); header[6] = 0xb0; header[12] = 60;
    header[13] = 0xf0; header[15] = 0xfc; header[16] = 0xfd; header[17] = 0xf8; header[18] = 0xf8; header[21] = 3; header[22] = 3;
    return Buffer.concat([header, ...[32, 33, 34].map((kind) => Buffer.from([kind | (complete ? 0x80 : 0), 0, 1, 0, 2, kind << 1, 1]))]);
}

function probe(bytes, codec = 'hevc', sampleEntry = 'hvc1', level = 60) {
    return { streams: [{ index: 0, codec_type: 'video', codec_name: codec, codec_tag_string: sampleEntry, level,
        extradata_size: bytes.length, extradata: `\n${bytes.toString('base64')}\n` }] };
}

test('each container retains its actual HEVC array completeness bytes and exact independent digest', () => {
    const mp4 = hvcc(true);
    const mkv = hvcc(false);
    const original = demuxedConfiguration(probe(mp4), 'hevc', 'mp4');
    const remux = demuxedConfiguration(probe(mkv, 'hevc', '[0][0][0][0]'), 'hevc', 'mkv');
    assert.equal(original.codecString, 'hvc1.1.6.L60.B0');
    assert.equal(remux.codecString, 'hev1.1.6.L60.B0');
    assert.equal(original.sampleEntry, 'hvc1');
    assert.equal(remux.sampleEntry, null);
    assert.equal(remux.configurationSource, 'ffprobe-demuxed-extradata');
    assert.equal(remux.configurationHex, mkv.toString('hex'));
    assert.equal(remux.configurationSha256, createHash('sha256').update(mkv).digest('hex'));
    assert.notEqual(remux.configurationSha256, original.configurationSha256);
    assert.notEqual(remux.configurationHex, original.configurationHex);
    assert.equal(remux.streamIndex, 0);
});

test('AV1 actual av1C stays independently bound across MP4, MKV and WebM', () => {
    const bytes = Buffer.from('81000c000a0e000000043cfecedaf99010101040', 'hex');
    for (const container of ['mp4', 'mkv', 'webm']) {
        const value = demuxedConfiguration(probe(bytes, 'av1', container === 'mp4' ? 'av01' : '[0][0][0][0]', 0), 'av1', container);
        assert.equal(value.codecString, 'av01.0.00M.08');
        assert.equal(value.configurationHex, bytes.toString('hex'));
        assert.equal(value.sampleEntry, container === 'mp4' ? 'av01' : null);
    }
});

test('source configuration rejects ambiguous, mismatched and guessed stream metadata', () => {
    const valid = probe(hvcc(true));
    const change = (mutate) => { const value = structuredClone(valid); mutate(value); return value; };
    for (const value of [
        change((value) => value.streams.push(value.streams[0])),
        change((value) => { value.streams[0].index = 1; }),
        change((value) => { value.streams[0].codec_type = 'audio'; }),
        change((value) => { value.streams[0].codec_name = 'av1'; }),
        change((value) => { value.streams[0].level = 63; }),
        change((value) => { delete value.streams[0].codec_tag_string; }),
        change((value) => { value.streams[0].codec_tag_string = 'av01'; }),
    ]) assert.throws(() => demuxedConfiguration(value, 'hevc', 'mp4'));
    assert.throws(() => demuxedConfiguration(valid, 'hevc', 'webm'));
});

test('configuration bounds and canonical Base64 reject truncated, excessive and malformed bytes', () => {
    const valid = probe(hvcc(true));
    for (const size of [0, 65537, valid.streams[0].extradata_size - 1]) {
        const value = structuredClone(valid); value.streams[0].extradata_size = size;
        assert.throws(() => demuxedConfiguration(value, 'hevc', 'mp4'));
    }
    for (const encoded of ['!', 'A===', 'AA', 'AB==', 'AA==\u00a0', 'A'.repeat(90000)]) {
        const value = structuredClone(valid); value.streams[0].extradata = encoded;
        assert.throws(() => demuxedConfiguration(value, 'hevc', 'mp4'));
    }
    const truncated = probe(hvcc(true).subarray(0, -1));
    assert.throws(() => demuxedConfiguration(truncated, 'hevc', 'mp4'));
});
