// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import assert from 'node:assert/strict';
import test from 'node:test';
import { audioProof, compareFramehash, compareMetadata, opusPacketProof, parseFramehash, validateHdr } from './format-proof.mjs';

function framehash(denominator, change = () => {}) {
    return `#tb 0: 1/${denominator}\n${Array.from({ length: 192 }, (_, index) => {
        const item = { pts: Math.round(index * denominator / 24), duration: Math.round(denominator / 24), hash: index.toString(16).padStart(64, '0') };
        change(item, index);
        return `0, ${item.pts}, ${item.pts}, ${item.duration}, 172800, ${item.hash}`;
    }).join('\n')}\n`;
}

test('copy proof normalizes MP4/Matroska rational clocks but retains exact native pixels', () => {
    const master = parseFramehash(framehash(12288));
    const remux = parseFramehash(framehash(1000));
    const proof = compareFramehash(master, remux);
    assert.equal(proof.frames, 192);
    assert.equal(proof.exactPixels, true);
    assert.ok(proof.maxTimingDeltaSeconds <= 0.001);
    assert.throws(() => compareFramehash(master, parseFramehash(framehash(1000, (item, index) => { if (index === 120) item.hash = 'f'.repeat(64); }))), /pixels/);
});

test('missing frames, repeated frames and wrong cadence cannot pass an aggregate hash', () => {
    assert.throws(() => parseFramehash(framehash(1000).replace(/0, 7958[^\n]+\n/, '')), /192/);
    assert.throws(() => parseFramehash(framehash(1000, (item, index) => { if (index > 100) item.pts += 8; })), /cadence/);
    assert.throws(() => parseFramehash(framehash(1000, (item, index) => { if (index === 100) item.pts = 4125; })), /cadence/);
    assert.throws(() => parseFramehash(framehash(24)), /precision/);
    assert.throws(() => parseFramehash(framehash(1000).replace('#tb 0: 1/1000', '#tb 0: 1/0')), /time base/);
});

test('decoded PCM proof distinguishes selected880 from alternate440 and refuses priming/sample loss', () => {
    const pcm = Buffer.alloc(48000 * 8 * 2);
    for (let sample = 0; sample < 48000 * 8; sample++) pcm.writeInt16LE(Math.round(Math.sin(sample * 2 * Math.PI * 880 / 48000) * 4000), sample * 2);
    assert.equal(audioProof(pcm, 880).hz, 880);
    assert.throws(() => audioProof(pcm, 440), /signal/);
    assert.throws(() => audioProof(pcm.subarray(2), 880), /duration/);
    assert.throws(() => audioProof(Buffer.alloc(pcm.length), 880), /signal/);
});

test('aligned Opus packet proof rejects missing pre-skip and any end-discard dependency', () => {
    const probe = { packets: Array.from({ length: 3201 }, () => ({ duration: 120 })) };
    probe.packets[0].side_data_list = [{ side_data_type: 'Skip Samples', skip_samples: 120, discard_padding: 0 }];
    assert.deepEqual(opusPacketProof(probe), { packetSamples: 120, packets: 3201, preSkip: 120, discardPadding: 0, encodedSamples: 384120, presentedSamples: 384000 });
    const padded = structuredClone(probe);
    padded.packets.at(-1).side_data_list = [{ side_data_type: 'Skip Samples', skip_samples: 0, discard_padding: 648 }];
    assert.throws(() => opusPacketProof(padded), /padding changed/);
    const missingSkip = structuredClone(probe);
    delete missingSkip.packets[0].side_data_list;
    assert.throws(() => opusPacketProof(missingSkip), /padding changed/);
    const variableDuration = structuredClone(probe);
    variableDuration.packets.at(-1).duration = 1;
    assert.throws(() => opusPacketProof(variableDuration), /duration changed/);
});

test('HDR metadata proof rejects relabelled SDR and altered content-light side data', () => {
    const reference = { maxCll: 1005, maxFall: 200 };
    const mastering = { side_data_type: 'Mastering display metadata', red_x: '35400/50000', red_y: '14600/50000', green_x: '8500/50000', green_y: '39850/50000', blue_x: '6550/50000', blue_y: '2300/50000', white_point_x: '15635/50000', white_point_y: '16450/50000', min_luminance: '1/10000', max_luminance: '10000000/10000' };
    const probe = { streams: [{}], firstVideoFrame: { side_data_list: [mastering, { side_data_type: 'Content light level metadata', max_content: 1005, max_average: 200 }] } };
    assert.equal(validateHdr(probe, { dynamicRange: 'hdr10' }, reference).length, 2);
    assert.throws(() => validateHdr(probe, { dynamicRange: 'sdr' }, reference));
    assert.throws(() => validateHdr({ firstVideoFrame: {} }, { dynamicRange: 'hdr10' }, reference));
    const changed = structuredClone(probe);
    changed.firstVideoFrame.side_data_list[1].max_average = 400;
    assert.throws(() => compareMetadata(probe, changed), /HDR/);
});
