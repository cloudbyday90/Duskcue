// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { mkdir, unlink, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { MASTERS, writeReference } from '../../docker/qualification/format-reference.mjs';
import { runBounded } from './commands.mjs';
import { hashFile } from './source.mjs';
import { encodingArguments } from '../../docker/qualification/format-corpus.mjs';

export async function createVodSource(source, directory, ffmpeg) {
    await mkdir(directory, { recursive: false });
    const raw = join(directory, 'reference.yuv');
    await writeReference(raw, MASTERS[0]);
    const captions = [join(directory, 'first.srt'), join(directory, 'second.srt')];
    for (const [index, text] of ['UNSELECTED CAPTION', 'DUSKCUE SELECTED CAPTION'].entries()) {
        await writeFile(captions[index], `1\n00:00:01,000 --> 00:00:07,000\n{\\an8}${text}\n\n`, { flag: 'wx' });
    }
    const path = join(directory, 'source.mkv');
    const result = await runBounded(ffmpeg, ['-hide_banner', '-v', 'error', '-nostdin', '-threads', '1', '-filter_threads', '1',
        '-f', 'rawvideo', '-pixel_format', 'yuv420p', '-video_size', '320x180', '-framerate', '24', '-i', raw, '-i', source,
        '-i', captions[0], '-i', captions[1],
        '-map', '1:0', '-map', '0:0', '-map', '1:2', '-map', '2:s:0', '-map', '3:s:0', '-frames:v', '192', '-t', '8', '-fs', '8388608',
        '-c:v', 'libx264', '-preset', 'veryfast', '-qp', '14', '-threads', '1', '-g', '48', '-bf', '3',
        '-x264-params', 'b-adapt=0:scenecut=0:open-gop=0', '-vf', 'setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709', '-color_primaries', 'bt709', '-color_trc', 'bt709', '-colorspace', 'bt709', '-color_range', 'tv',
        '-c:a', 'copy', '-c:s', 'copy', '-disposition:a:0', '0', '-disposition:a:1', 'default',
        '-disposition:s:0', '0', '-disposition:s:1', 'hearing_impaired', '-metadata:s:s:1', 'language=fra', '-n', path], { timeoutMs: 30000, bytes: 1048576 });
    if (result.code !== 0 || result.signal) throw new Error('Owned VOD timing source creation failed.');
    const observed = await runBounded(join(dirname(ffmpeg), ffmpeg.endsWith('.exe') ? 'ffprobe.exe' : 'ffprobe'), ['-v', 'error', '-select_streams', 'v:0', '-show_streams', '-of', 'json', path], { timeoutMs: 10000, bytes: 1048576 });
    const video = JSON.parse(observed.stdout).streams?.[0];
    if (observed.code !== 0 || video?.color_primaries !== 'bt709' || video.color_transfer !== 'bt709' || video.color_space !== 'bt709' || video.color_range !== 'tv') throw new Error('Owned VOD source did not preserve its authored BT.709 signalling.');
    await unlink(raw);
    for (const caption of captions) await unlink(caption);
    const copyPath = join(directory, 'source.mp4');
    const copy = await runBounded(ffmpeg, ['-v', 'error', '-nostdin', '-i', path, '-map', '0:0', '-map', '0:1', '-map', '0:2', '-c', 'copy', '-movflags', '+faststart', '-disposition:a:0', '0', '-disposition:a:1', 'default', '-metadata:s:a:0', 'language=eng', '-metadata:s:a:1', 'language=fra', '-n', copyPath], { timeoutMs: 30000, bytes: 1048576 });
    if (copy.code !== 0 || copy.signal) throw new Error('Owned VOD copy source creation failed.');
    const variants = [];
    for (const master of MASTERS.filter((m) => m.dynamicRange === 'sdr')) {
        const raw = join(directory, `${master.id}.yuv`);
        const reference = await writeReference(raw, master);
        const path = join(directory, `${master.id}.mp4`);
        const encoded = await runBounded(ffmpeg, ['-hide_banner', '-v', 'error', '-nostdin', '-threads', '1', '-filter_threads', '1',
            '-f', 'rawvideo', '-pixel_format', reference.pixelFormat, '-video_size', '320x180', '-framerate', '24', '-i', raw, '-i', source,
            '-map', '1:0', '-map', '0:0', '-map', '1:2', '-frames:v', '192', '-t', '8', '-fs', '8388608',
            ...encodingArguments(master, reference), '-c:a', 'copy', '-disposition:a:0', '0', '-disposition:a:1', 'default', '-movflags', '+faststart', '-n', path], { timeoutMs: 30000, bytes: 1048576 });
        if (encoded.code !== 0 || encoded.signal) throw new Error(`Owned VOD ${master.id} source creation failed.`);
        await unlink(raw);
        variants.push({ id: master.id, path, sha256: await hashFile(path) });
    }
    const corpus = join(directory, 'corpus.json');
    await writeFile(corpus, `${JSON.stringify(variants)}\n`, { flag: 'wx' });
    return { path, sha256: await hashFile(path), copyPath, copySha256: await hashFile(copyPath), corpus, variants, video: 'authored frame IDs at 24 fps, BT.709 SDR', audio: 'original selected 880-Hz stream', subtitles: 'two top-aligned SRT tracks clear of the frame-ID band' };
}
