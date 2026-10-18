// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { open, unlink } from 'node:fs/promises';
import { basename } from 'node:path';
import { boundedMedia, mediaPrelude } from './format-process.mjs';
import { WIDTH, HEIGHT, FRAME_RATE, FRAMES } from './format-reference.mjs';

export function parseFramehash(text) {
    const timebase = /^#tb 0: (\d+)\/(\d+)\s*$/m.exec(text);
    if (!timebase || Number(timebase[1]) < 1 || Number(timebase[2]) < 1) throw new Error('Invalid framehash time base.');
    const frames = text.split(/\r?\n/).filter((line) => line.trim() && !line.startsWith('#')).map((line) => {
        const fields = line.split(',').map((field) => field.trim());
        if (fields.length !== 6 || fields[0] !== '0' || !fields.slice(1, 5).every((value) => /^-?\d+$/.test(value) && Number.isSafeInteger(Number(value))) || !/^[a-f0-9]{64}$/.test(fields[5])) throw new Error('Invalid decoded framehash record.');
        const [dts, pts, duration, bytes] = fields.slice(1, 5).map(Number);
        if (duration < 0 || bytes < 1) throw new Error('Invalid framehash duration or size.');
        return { dts, pts, duration, bytes, sha256: fields[5] };
    });
    if (frames.length !== FRAMES) throw new Error(`Expected ${FRAMES} decoded frames, received ${frames.length}.`);
    const result = { timebase: { numerator: Number(timebase[1]), denominator: Number(timebase[2]) }, frames };
    const tick = result.timebase.numerator / result.timebase.denominator;
    if (tick > 0.001001) throw new Error('Unexpected decoded timing precision.');
    for (let index = 0; index < frames.length; index++) {
        const relative = (frames[index].pts - frames[0].pts) * tick;
        if (Math.abs(relative - index / FRAME_RATE) > tick + 1e-8 || frames[index].dts !== frames[index].pts || index && frames[index].pts <= frames[index - 1].pts) throw new Error(`Decoded frame cadence changed at frame ${index}.`);
    }
    return result;
}

export function compareFramehash(master, remux) {
    const masterTick = master.timebase.numerator / master.timebase.denominator;
    const remuxTick = remux.timebase.numerator / remux.timebase.denominator;
    if (master.frames.length !== remux.frames.length) throw new Error('Remux frame count changed.');
    let maxTimingDeltaSeconds = 0;
    for (let index = 0; index < master.frames.length; index++) {
        const expected = master.frames[index];
        const actual = remux.frames[index];
        if (expected.sha256 !== actual.sha256 || expected.bytes !== actual.bytes) throw new Error(`Remux decoded pixels changed at frame ${index}.`);
        const delta = Math.abs((actual.pts - remux.frames[0].pts) * remuxTick - (expected.pts - master.frames[0].pts) * masterTick);
        const durationDelta = Math.abs(actual.duration * remuxTick - expected.duration * masterTick);
        maxTimingDeltaSeconds = Math.max(maxTimingDeltaSeconds, delta, durationDelta);
        if (delta > Math.max(masterTick, remuxTick) + 1e-8 || durationDelta > Math.max(masterTick, remuxTick) + 1e-8) throw new Error(`Remux timing changed beyond its clock quantum at frame ${index}.`);
    }
    return { exactPixels: true, frames: master.frames.length, maxTimingDeltaSeconds, masterTimebase: master.timebase, remuxTimebase: remux.timebase, relativeTiming: true };
}

export async function decodedVideo(ffmpeg, path, pixelFormat) {
    return parseFramehash((await boundedMedia(ffmpeg, [...mediaPrelude, '-copyts', '-i', path, '-map', '0:v:0', '-an', '-sn', '-c:v', 'rawvideo', '-threads', '1', '-pix_fmt', pixelFormat, '-fps_mode', 'passthrough', '-enc_time_base', 'demux', '-f', 'framehash', '-hash', 'sha256', 'pipe:1'])).toString('utf8'));
}

export function audioProof(pcm, expectedHz) {
    if (pcm.length !== 48000 * 8 * 2) throw new Error(`Decoded Opus duration changed (${pcm.length} PCM bytes).`);
    let crossings = 0;
    let peak = 0;
    const start = 48000;
    const count = 48000;
    let previous = pcm.readInt16LE((start - 1) * 2);
    for (let index = start; index < start + count; index++) {
        const sample = pcm.readInt16LE(index * 2);
        if (previous <= 0 && sample > 0) crossings++;
        peak = Math.max(peak, Math.abs(sample));
        previous = sample;
    }
    const hz = crossings;
    if (peak < 500 || Math.abs(hz - expectedHz) > 2) throw new Error('Decoded selected audio signal changed.');
    return { sha256: createHash('sha256').update(pcm).digest('hex'), bytes: pcm.length, samples: pcm.length / 2, sampleRate: 48000, channels: 1, hz };
}

export async function decodedAudio(ffmpeg, path, index, hz) {
    try {
        return audioProof(await boundedMedia(ffmpeg, [...mediaPrelude, '-i', path, '-map', `0:${index}`, '-vn', '-sn', '-c:a', 'pcm_s16le', '-threads', '1', '-ar', '48000', '-ac', '1', '-f', 's16le', 'pipe:1']), hz);
    } catch (error) {
        throw new Error(`Opus PCM proof failed for ${basename(path)} stream ${index} (${hz} Hz, expected 384000 samples): ${error.message}`, { cause: error });
    }
}

export function opusPacketProof(probe) {
    const packets = probe.packets;
    if (!Array.isArray(packets) || packets.length !== 3201) throw new Error('Aligned Opus source packet count changed.');
    let preSkip = 0;
    let discardPadding = 0;
    for (const [index, packet] of packets.entries()) {
        if (Number(packet.duration) !== 120) throw new Error('Aligned Opus source packet duration changed.');
        for (const side of packet.side_data_list || []) {
            if (side.side_data_type !== 'Skip Samples') continue;
            const skip = Number(side.skip_samples);
            const discard = Number(side.discard_padding);
            if (!Number.isSafeInteger(skip) || skip < 0 || !Number.isSafeInteger(discard) || discard < 0 || index !== 0 && skip !== 0) throw new Error('Invalid aligned Opus padding side data.');
            preSkip += skip;
            discardPadding += discard;
        }
    }
    const encodedSamples = packets.length * 120;
    if (preSkip !== 120 || discardPadding !== 0 || encodedSamples - preSkip !== 384000) throw new Error(`Aligned Opus source padding changed: ${JSON.stringify({ preSkip, discardPadding, encodedSamples })}`);
    return { packetSamples: 120, packets: packets.length, preSkip, discardPadding, encodedSamples, presentedSamples: encodedSamples - preSkip };
}

export async function probeOpusPadding(ffprobe, path) {
    return opusPacketProof(JSON.parse((await boundedMedia(ffprobe, ['-v', 'error', '-select_streams', 'a:0', '-show_packets', '-show_entries', 'packet=duration:packet_side_data=side_data_type,skip_samples,discard_padding', '-of', 'json', path])).toString('utf8')));
}

export async function verifyReference(ffmpeg, path, decodedPath, master, reference, hashes) {
    const frameBytes = WIDTH * HEIGHT * 3 / 2 * (master.bitDepth === 10 ? 2 : 1);
    if (master.codec === 'av1' && hashes.frames.some((frame, index) => frame.sha256 !== reference.frameSha256[index])) throw new Error('Lossless AV1 differs from authored native-precision pixels.');
    await boundedMedia(ffmpeg, [...mediaPrelude, '-i', path, '-map', '0:v:0', '-an', '-sn', '-c:v', 'rawvideo', '-threads', '1', '-pix_fmt', reference.pixelFormat, '-fps_mode', 'passthrough', '-f', 'rawvideo', '-fs', String(frameBytes * FRAMES + 1), '-n', decodedPath]);
    let maxError = 0;
    let precisionSensitiveSamples = 0;
    const file = await open(decodedPath, 'r');
    try {
        if ((await file.stat()).size !== frameBytes * FRAMES) throw new Error('Decoded reference frame byte count changed.');
        const bytes = Buffer.alloc(frameBytes);
        for (let frame = 0; frame < FRAMES; frame++) {
            let offset = 0;
            while (offset < bytes.length) {
                const read = await file.read(bytes, offset, bytes.length - offset);
                if (read.bytesRead < 1) throw new Error('Decoded reference was truncated.');
                offset += read.bytesRead;
            }
            for (let patch = 0; patch < reference.patches.length; patch++) for (let y = 24; y < 56; y++) for (let x = 24 + patch * 32; x < 40 + patch * 32; x++) {
                const pixel = y * WIDTH + x;
                const actual = master.bitDepth === 10 ? bytes.readUInt16LE(pixel * 2) : bytes[pixel];
                const error = Math.abs(actual - reference.patches[patch].code);
                maxError = Math.max(maxError, error);
                if (error > reference.toleranceCodes) throw new Error(`Numeric reference patch ${patch} frame ${frame} differs by ${error} native codes (limit ${reference.toleranceCodes}).`);
                if (master.bitDepth === 10 && actual % 4 !== 0) precisionSensitiveSamples++;
            }
        }
        if (master.bitDepth === 10 && !precisionSensitiveSamples) throw new Error('Ten-bit precision-sensitive samples were lost.');
    } finally { await file.close(); await unlink(decodedPath); }
    return { checkedFrames: FRAMES, maxErrorCodes: maxError, toleranceCodes: reference.toleranceCodes, precisionSensitiveSamples, rawPixelExact: master.codec === 'av1' };
}

export async function probeSource(ffprobe, path) {
    const probe = JSON.parse((await boundedMedia(ffprobe, ['-v', 'error', '-threads', '1', '-show_streams', '-show_format', '-of', 'json', path])).toString('utf8'));
    const first = JSON.parse((await boundedMedia(ffprobe, ['-v', 'error', '-threads', '1', '-select_streams', 'v:0', '-read_intervals', '%+#1', '-show_frames', '-show_entries', 'frame=color_range,color_space,color_primaries,color_transfer:frame_side_data', '-of', 'json', path])).toString('utf8'));
    if (first.frames?.length !== 1) throw new Error('First decoded reference frame metadata is unavailable.');
    return { ...probe, firstVideoFrame: first.frames[0] };
}

export function validateProbe(probe, master) {
    const video = probe.streams?.[0];
    const transfer = master.dynamicRange === 'hdr10' ? 'smpte2084' : master.dynamicRange === 'hlg' ? 'arib-std-b67' : 'bt709';
    const primaries = master.dynamicRange === 'sdr' ? 'bt709' : 'bt2020';
    const matrix = master.dynamicRange === 'sdr' ? 'bt709' : 'bt2020nc';
    const profile = master.codec === 'hevc' ? master.bitDepth === 10 ? 'Main 10' : 'Main' : 'Main';
    if (probe.streams?.length !== 3 || video.index !== 0 || video.codec_name !== master.codec || video.profile !== profile || video.pix_fmt !== (master.bitDepth === 10 ? 'yuv420p10le' : 'yuv420p') || video.width !== WIDTH || video.height !== HEIGHT || video.avg_frame_rate !== '24/1' || video.color_transfer !== transfer || video.color_primaries !== primaries || video.color_space !== matrix || video.color_range !== 'tv' || !Number.isInteger(video.level) || video.level < 0) {
        const keys = ['index', 'codec_name', 'profile', 'pix_fmt', 'width', 'height', 'avg_frame_rate', 'color_transfer', 'color_primaries', 'color_space', 'color_range', 'level'];
        throw new Error(`Reference video descriptor differs: ${JSON.stringify({ master: master.id, streams: probe.streams?.length, expected: { profile, transfer, primaries, matrix, bitDepth: master.bitDepth, width: WIDTH, height: HEIGHT, frameRate: FRAME_RATE }, actual: Object.fromEntries(keys.map((key) => [key, video?.[key] ?? null])) })}`);
    }
    for (let index = 1; index <= 2; index++) {
        const audio = probe.streams[index];
        if (audio.index !== index || audio.codec_name !== 'opus' || audio.channels !== 1 || Number(audio.sample_rate) !== 48000 || audio.disposition?.default !== (index === 2 ? 1 : 0)) throw new Error('Reference Opus track layout/default changed.');
    }
    const duration = Number(probe.format?.duration);
    if (!Number.isFinite(duration) || Math.abs(duration - 8) > 0.05) throw new Error('Reference source duration changed.');
    return video;
}

export function hdrMetadata(probe) {
    return (probe.firstVideoFrame?.side_data_list || []).filter((data) => ['Mastering display metadata', 'Content light level metadata'].includes(data.side_data_type));
}

export function validateHdr(probe, master, reference) {
    const side = hdrMetadata(probe);
    if (master.dynamicRange !== 'hdr10') {
        if (side.length) throw new Error('Non-PQ source unexpectedly claims HDR10 static mastering metadata.');
        return side;
    }
    const mastering = side.find((entry) => entry.side_data_type === 'Mastering display metadata');
    const light = side.find((entry) => entry.side_data_type === 'Content light level metadata');
    const fraction = (value) => { const parts = String(value).split('/').map(Number); return parts.length === 2 && parts[1] > 0 ? parts[0] / parts[1] : NaN; };
    const expected = { red_x: 0.708, red_y: 0.292, green_x: 0.17, green_y: 0.797, blue_x: 0.131, blue_y: 0.046, white_point_x: 0.3127, white_point_y: 0.329, min_luminance: 0.0001, max_luminance: 1000 };
    if (!mastering || !light || Object.entries(expected).some(([key, value]) => !Number.isFinite(fraction(mastering[key])) || Math.abs(fraction(mastering[key]) - value) > 1e-8) || light.max_content !== reference.maxCll || light.max_average !== reference.maxFall) throw new Error('PQ mastering/actual content-light SEI changed.');
    return side;
}

export function compareMetadata(master, remux) {
    const keys = ['codec_name', 'profile', 'level', 'width', 'height', 'pix_fmt', 'color_range', 'color_space', 'color_transfer', 'color_primaries', 'chroma_location'];
    for (const key of keys) if (master.streams[0][key] !== remux.streams[0][key]) throw new Error(`Remux changed video metadata ${key}.`);
    if (JSON.stringify(hdrMetadata(master)) !== JSON.stringify(hdrMetadata(remux))) throw new Error('Remux changed HDR mastering/content-light SEI.');
    return { video: true, hdrSideData: true, selectedAudioIndex: 2, selectedAudioDefault: true };
}
