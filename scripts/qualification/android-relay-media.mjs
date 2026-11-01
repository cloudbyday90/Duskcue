// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { mkdir, unlink } from 'node:fs/promises';
import { join } from 'node:path';
import { MASTERS, writeReference } from '../../docker/qualification/format-reference.mjs';
import { runBounded } from './commands.mjs';
import { hashFile } from './source.mjs';

export async function createAndroidRelayMedia(directory, ffmpeg, ffprobe) {
    await mkdir(directory);
    const raw = join(directory, 'authored.yuv'), path = join(directory, 'source.mp4');
    await writeReference(raw, MASTERS[0]);
    const made = await runBounded(ffmpeg, ['-nostdin', '-hide_banner', '-v', 'error', '-threads', '1', '-filter_threads', '1',
        '-f', 'rawvideo', '-pixel_format', 'yuv420p', '-video_size', '320x180', '-framerate', '24', '-i', raw,
        '-f', 'lavfi', '-i', 'aevalsrc=0.1*sin(2*PI*440*t)|0.1*sin(2*PI*880*t):s=48000:d=8',
        '-map', '0:v:0', '-map', '1:a:0', '-t', '8', '-frames:v', '192', '-fs', '8388608',
        '-vf', 'scale=1920:1080:flags=neighbor,format=yuv420p,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709',
        '-c:v', 'libx264', '-preset', 'veryfast', '-qp', '20', '-threads', '1', '-g', '48', '-bf', '3',
        '-x264-params', 'b-adapt=0:scenecut=0:open-gop=0', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709', '-color_range', 'tv',
        '-c:a', 'aac', '-ac', '2', '-b:a', '192k', '-movflags', '+faststart', '-n', path], { timeoutMs: 60000, bytes: 1048576 });
    if (made.code !== 0 || made.signal) throw new Error('Relay format source creation failed.');
    const probed = await runBounded(ffprobe, ['-v', 'error', '-show_streams', '-of', 'json', path], { timeoutMs: 10000, bytes: 65536 });
    const streams = JSON.parse(probed.stdout).streams;
    const video = streams?.find((s) => s.codec_type === 'video'), audio = streams?.find((s) => s.codec_type === 'audio');
    if (probed.code !== 0 || video?.width !== 1920 || video.height !== 1080 || video.codec_name !== 'h264'
        || video.color_primaries !== 'bt709' || video.color_transfer !== 'bt709' || video.color_space !== 'bt709'
        || audio?.codec_name !== 'aac' || audio.channels !== 2) throw new Error('Authored relay source format differs.');
    await unlink(raw);
    return { path, sha256: await hashFile(path), width: video.width, height: video.height, channels: audio.channels };
}
