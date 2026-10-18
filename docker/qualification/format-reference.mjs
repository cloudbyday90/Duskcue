// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { open } from 'node:fs/promises';

export const WIDTH = 320;
export const HEIGHT = 180;
export const FRAME_RATE = 24;
export const FRAMES = 192;
export const FILE_LIMIT = 8388608;
export const FRAME_ID = Object.freeze({ x: 16, y: 160, cellWidth: 32, cellHeight: 16, bits: 8, order: 'least-significant-first', sampleInsetX: 8, sampleInsetY: 4 });
export const MASTERS = Object.freeze([
    { id: 'hevc-sdr-8', codec: 'hevc', bitDepth: 8, dynamicRange: 'sdr' },
    { id: 'hevc-sdr-10', codec: 'hevc', bitDepth: 10, dynamicRange: 'sdr' },
    { id: 'av1-sdr-8', codec: 'av1', bitDepth: 8, dynamicRange: 'sdr' },
    { id: 'av1-sdr-10', codec: 'av1', bitDepth: 10, dynamicRange: 'sdr' },
    { id: 'hevc-pq-10', codec: 'hevc', bitDepth: 10, dynamicRange: 'hdr10' },
    { id: 'hevc-hlg-10', codec: 'hevc', bitDepth: 10, dynamicRange: 'hlg' },
]);

export function pqInverseEotf(nits) {
    if (!Number.isFinite(nits) || nits < 0 || nits > 10000) throw new Error('Invalid PQ luminance.');
    const power = (nits / 10000) ** (2610 / 16384);
    return ((3424 / 4096 + (2413 / 128) * power) / (1 + (2392 / 128) * power)) ** (2523 / 32);
}

export function pqEotf(signal) {
    if (!Number.isFinite(signal) || signal < 0 || signal > 1) throw new Error('Invalid PQ signal.');
    const power = signal ** (32 / 2523);
    return 10000 * (Math.max(power - 3424 / 4096, 0) / (2413 / 128 - (2392 / 128) * power)) ** (16384 / 2610);
}

export function hlgOetf(sceneLinear) {
    if (!Number.isFinite(sceneLinear) || sceneLinear < 0 || sceneLinear > 1) throw new Error('Invalid HLG scene value.');
    const a = 0.17883277;
    const b = 1 - 4 * a;
    const c = 0.5 - a * Math.log(4 * a);
    return sceneLinear <= 1 / 12 ? Math.sqrt(3 * sceneLinear) : Math.min(1, a * Math.log(12 * sceneLinear - b) + c);
}

export function narrowCode(signal, bitDepth) {
    if (![8, 10].includes(bitDepth) || !Number.isFinite(signal) || signal < 0 || signal > 1) throw new Error('Invalid narrow-range sample.');
    const scale = 2 ** (bitDepth - 8);
    return Math.round((16 + 219 * signal) * scale);
}

export function referencePatches(master) {
    if (master.dynamicRange === 'hdr10') return [0, 0.1, 1, 10, 100, 203, 400, 1000].map((nits) => ({ input: nits, unit: 'cd/m2', signal: pqInverseEotf(nits), code: narrowCode(pqInverseEotf(nits), 10) }));
    if (master.dynamicRange === 'hlg') return [0, 1 / 1024, 1 / 64, 1 / 12, 0.18, 0.5, 0.75, 1].map((scene) => ({ input: scene, unit: 'scene-linear', signal: hlgOetf(scene), code: narrowCode(hlgOetf(scene), 10) }));
    return [0, 1 / 16, 1 / 8, 1 / 4, 1 / 2, 3 / 4, 7 / 8, 1].map((signal) => ({ input: signal, unit: 'nonlinear-bt709', signal, code: narrowCode(signal, master.bitDepth) }));
}

export function referenceFrame(master, frame) {
    if (!MASTERS.some((candidate) => candidate.id === master.id && candidate.bitDepth === master.bitDepth && candidate.dynamicRange === master.dynamicRange) || !Number.isInteger(frame) || frame < 0 || frame >= FRAMES) throw new Error('Invalid reference frame.');
    const bytes = master.bitDepth === 10 ? 2 : 1;
    const samples = WIDTH * HEIGHT;
    const output = Buffer.alloc(samples * 3 / 2 * bytes);
    const patches = referencePatches(master);
    const black = narrowCode(0, master.bitDepth);
    const white = master.dynamicRange === 'hdr10' ? patches.at(-1).code : narrowCode(1, master.bitDepth);
    const write = (index, value) => { if (bytes === 2) output.writeUInt16LE(value, index * 2); else output[index] = value; };
    for (let y = 0; y < HEIGHT; y++) for (let x = 0; x < WIDTH; x++) {
        let value = black + Math.floor((white - black) * ((x + y) % 320) / 319);
        if (x >= 16 && x < 272 && y >= 16 && y < 64) value = patches[Math.floor((x - 16) / 32)].code;
        const moving = (frame * 3) % (WIDTH - 32);
        if (y >= 132 && y < 156 && x >= moving && x < moving + 32) value = white;
        if (x >= FRAME_ID.x && x < FRAME_ID.x + FRAME_ID.bits * FRAME_ID.cellWidth && y >= FRAME_ID.y && y < FRAME_ID.y + FRAME_ID.cellHeight) value = frame & (1 << Math.floor((x - FRAME_ID.x) / FRAME_ID.cellWidth)) ? white : black;
        write(y * WIDTH + x, value);
    }
    for (let index = samples; index < samples * 3 / 2; index++) write(index, 128 * 2 ** (master.bitDepth - 8));
    return output;
}

export async function writeReference(path, master) {
    const file = await open(path, 'wx', 0o600);
    const hashes = [];
    let maxFall = 0;
    let maxCll = 0;
    try {
        for (let frame = 0; frame < FRAMES; frame++) {
            const bytes = referenceFrame(master, frame);
            hashes.push(createHash('sha256').update(bytes).digest('hex'));
            let written = 0;
            while (written < bytes.length) {
                const result = await file.write(bytes, written, bytes.length - written);
                if (result.bytesWritten < 1) throw new Error('Reference frame write made no progress.');
                written += result.bytesWritten;
            }
            if (master.dynamicRange === 'hdr10') {
                let sum = 0;
                for (let pixel = 0; pixel < WIDTH * HEIGHT; pixel++) {
                    const luminance = pqEotf((bytes.readUInt16LE(pixel * 2) - 64) / 876);
                    sum += luminance;
                    maxCll = Math.max(maxCll, luminance);
                }
                maxFall = Math.max(maxFall, sum / (WIDTH * HEIGHT));
            }
        }
    } finally { await file.close(); }
    return { standard: master.dynamicRange === 'sdr' ? 'BT.709 narrow-range diagnostic' : 'ITU-R BT.2100-3 (2025-02)', toleranceCodes: master.codec === 'hevc' ? 2 : 0, scope: 'neutral grayscale transfer/range; no chromatic gamut or physical display claim', pixelFormat: master.bitDepth === 10 ? 'yuv420p10le' : 'yuv420p', frames: FRAMES, frameRate: FRAME_RATE, frameId: FRAME_ID, patchGeometry: { x: 16, y: 16, width: 32, height: 48, sampleInsetX: 8, sampleInsetY: 8 }, patches: referencePatches(master), frameSha256: hashes, ...(master.dynamicRange === 'hdr10' ? { maxCll: Math.ceil(maxCll), maxFall: Math.ceil(maxFall), masteringDisplay: 'G(8500,39850)B(6550,2300)R(35400,14600)WP(15635,16450)L(10000000,1)' } : {}) };
}
