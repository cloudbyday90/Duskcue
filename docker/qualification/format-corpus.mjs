// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { lstat, mkdir, mkdtemp, open, readFile, readdir, realpath, rm, unlink } from 'node:fs/promises';
import { basename, dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mp4CodecConfiguration } from '../../scripts/qualification/format-codecs.mjs';
import { boundedMedia, fileIdentity, mediaPrelude, toolIdentity } from './format-process.mjs';
import { probeConfiguration } from './format-configuration.mjs';
import { FILE_LIMIT, FRAME_RATE, FRAMES, HEIGHT, MASTERS, WIDTH, writeReference } from './format-reference.mjs';
import { compareFramehash, compareMetadata, decodedAudio, decodedVideo, probeOpusPadding, probeSource, validateHdr, validateProbe, verifyReference } from './format-proof.mjs';

export function audioEncodingArguments() {
    return ['-c:a', 'libopus', '-application', 'lowdelay', '-frame_duration', '2.5', '-threads', '1', '-b:a', '96000', '-vbr', 'off', '-compression_level', '0', '-ac', '1', '-ar', '48000'];
}

export function encodingArguments(master, reference) {
    const color = master.dynamicRange === 'sdr' ? ['bt709', 'bt709', 'bt709'] : ['bt2020', master.dynamicRange === 'hdr10' ? 'smpte2084' : 'arib-std-b67', 'bt2020nc'];
    const common = ['-threads', '1', '-pix_fmt', reference.pixelFormat, '-g', '48', '-keyint_min', '48', '-vf', `setparams=range=limited:color_primaries=${color[0]}:color_trc=${color[1]}:colorspace=${color[2]}`, '-color_primaries', color[0], '-color_trc', color[1], '-colorspace', color[2], '-color_range', 'tv'];
    if (master.codec === 'hevc') {
        const params = ['qp=1', 'ipratio=1', 'pbratio=1', 'lossless=0', 'pools=none', 'frame-threads=1', 'lookahead-threads=1', 'wpp=0', 'keyint=48', 'min-keyint=48', 'scenecut=0', 'bframes=0', 'rc-lookahead=0', 'open-gop=0', 'sao=0', 'no-deblock=1', 'strong-intra-smoothing=0', 'psy-rd=0', 'psy-rdoq=0'];
        if (master.dynamicRange === 'hdr10') params.push(`master-display=${reference.masteringDisplay}`, `max-cll=${reference.maxCll},${reference.maxFall}`, 'hdr10=1', 'hdr10-opt=0');
        return ['-c:v', 'libx265', '-preset', 'ultrafast', '-profile:v', master.bitDepth === 10 ? 'main10' : 'main', '-x265-params', params.join(':'), '-tag:v', 'hvc1', ...common];
    }
    if (master.codec === 'av1') return ['-c:v', 'libaom-av1', '-profile:v', '0', '-cpu-used', '8', '-crf', '0', '-b:v', '0', '-aom-params', 'lossless=1', '-lag-in-frames', '0', '-auto-alt-ref', '0', '-row-mt', '0', '-tiles', '1x1', '-frame-parallel', '0', '-tag:v', 'av01', ...common];
    throw new Error('Unsupported reference encoder.');
}

function capabilityFor(container, configuration, probe, bitrate, audioBitrate, dynamicRange) {
    const type = container === 'mp4' ? 'mp4' : container === 'webm' ? 'webm' : 'x-matroska';
    const video = { contentType: `video/${type}; codecs="${configuration.codecString}"`, width: WIDTH, height: HEIGHT, bitrate, framerate: FRAME_RATE, colorGamut: dynamicRange === 'sdr' ? 'srgb' : 'rec2020' };
    if (dynamicRange !== 'sdr') video.transferFunction = dynamicRange === 'hdr10' ? 'pq' : 'hlg';
    if (probe.firstVideoFrame?.side_data_list?.some((side) => side.side_data_type === 'Mastering display metadata')) video.hdrMetadataType = 'smpteSt2086';
    const audio = { contentType: `audio/${type}; codecs="opus"`, channels: '1', bitrate: audioBitrate, samplerate: 48000 };
    return { type: 'file', video, audio };
}

async function ownedDirectory(directory) {
    if (!isAbsolute(directory) || resolve(directory) !== directory || dirname(directory) === directory) throw new Error('An exact absolute owned corpus directory is required.');
    try { await mkdir(directory, { mode: 0o700 }); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    const metadata = await lstat(directory);
    if (!metadata.isDirectory() || metadata.isSymbolicLink() || await realpath(directory) !== directory || (await readdir(directory)).length) throw new Error('Corpus directory must be canonical, regular and empty.');
    return directory;
}

export async function generateFormatCorpus({ directory, ffmpeg = 'ffmpeg', ffprobe = 'ffprobe' }) {
    await ownedDirectory(directory);
    const manifestPath = join(dirname(directory), 'format-corpus.json');
    try { await lstat(manifestPath); throw new Error('Corpus manifest already exists.'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    const work = await mkdtemp(join(directory, '.format-work-'));
    const created = [];
    let completed = false;
    let manifestCreated = false;
    try {
        const tools = { ffmpeg: await toolIdentity(ffmpeg), ffprobe: await toolIdentity(ffprobe) };
        ffmpeg = tools.ffmpeg.path;
        ffprobe = tools.ffprobe.path;
        tools.encoders = {};
        for (const encoder of ['libx265', 'libaom-av1', 'libopus']) {
            const help = (await boundedMedia(ffmpeg, ['-hide_banner', '-h', `encoder=${encoder}`])).toString('utf8');
            if (!help.includes(`Encoder ${encoder} `)) throw new Error(`Qualified encoder ${encoder} is unavailable.`);
            if (encoder !== 'libopus' && (!help.includes('yuv420p ') || !help.includes('yuv420p10le'))) throw new Error(`Qualified encoder ${encoder} lacks eight/ten-bit planar support.`);
            tools.encoders[encoder] = help.trim();
        }
        const tones = [];
        for (const hz of [440, 880]) {
            const path = join(work, `tone-${hz}.opus`);
            await boundedMedia(ffmpeg, [...mediaPrelude, '-f', 'lavfi', '-i', `sine=frequency=${hz}:sample_rate=48000:duration=8`, '-map', '0:a:0', ...audioEncodingArguments(), '-t', '8', '-fs', String(FILE_LIMIT), '-n', path]);
            const identity = await fileIdentity(path, FILE_LIMIT);
            tones.push({ path, hz, ...identity, bitrate: Math.ceil(identity.bytes * 8 / 8), encoderArguments: audioEncodingArguments(), padding: await probeOpusPadding(ffprobe, path), pcm: await decodedAudio(ffmpeg, path, 0, hz) });
        }
        const cases = [];
        const masters = [];
        for (const master of MASTERS) {
            const raw = join(work, `${master.id}.yuv`);
            const reference = await writeReference(raw, master);
            const filename = `${master.id}-mp4.mp4`;
            const path = join(directory, filename);
            created.push(path);
            const encoderArguments = encodingArguments(master, reference);
            await boundedMedia(ffmpeg, [...mediaPrelude, '-f', 'rawvideo', '-pixel_format', reference.pixelFormat, '-video_size', `${WIDTH}x${HEIGHT}`, '-framerate', String(FRAME_RATE), '-i', raw, '-i', tones[0].path, '-i', tones[1].path, '-map', '0:v:0', '-map', '1:a:0', '-map', '2:a:0', '-map_metadata', '-1', '-map_chapters', '-1', ...encoderArguments, '-c:a', 'copy', '-disposition:a:0', '0', '-disposition:a:1', 'default', '-metadata:s:a:0', 'language=eng', '-metadata:s:a:1', 'language=eng', '-metadata:s:a:0', 'title=Alternate 440 Hz', '-metadata:s:a:1', 'title=Default 880 Hz', '-frames:v', String(FRAMES), '-t', '8', '-movflags', '+faststart', '-fs', String(FILE_LIMIT), '-n', path], { timeoutMs: 90000 });
            await unlink(raw);
            const masterIdentity = await fileIdentity(path, FILE_LIMIT);
            const masterProbe = await probeSource(ffprobe, path);
            const video = validateProbe(masterProbe, master);
            validateHdr(masterProbe, master, reference);
            const configuration = mp4CodecConfiguration(await readFile(path));
            if (configuration.codec !== master.codec || configuration.bitDepth !== master.bitDepth || configuration.level !== video.level || master.codec === 'hevc' && (configuration.profile !== (master.bitDepth === 8 ? 1 : 2) || configuration.level > 186)) throw new Error('Actual master decoder configuration differs from the declared ordinary profile.');
            const decoded = await decodedVideo(ffmpeg, path, reference.pixelFormat);
            const numeric = await verifyReference(ffmpeg, path, join(work, `${master.id}-decoded.yuv`), master, reference, decoded);
            const audio = [];
            for (const [index, hz] of [[1, 440], [2, 880]]) {
                const proof = await decodedAudio(ffmpeg, path, index, hz);
                if (proof.sha256 !== tones[index - 1].pcm.sha256 || proof.bytes !== tones[index - 1].pcm.bytes) throw new Error('MP4 changed decoded companion Opus PCM.');
                audio.push(proof);
            }
            const actualBitrate = Number(video.bit_rate);
            const bitrate = Number.isSafeInteger(actualBitrate) && actualBitrate > 0 ? actualBitrate : Math.ceil(masterIdentity.bytes * 8 / 8);
            const masterProof = { id: master.id, encoder: master.codec === 'hevc' ? 'libx265' : 'libaom-av1', encoderArguments, reference, numeric, configuration, decoded, audio, sha256: masterIdentity.sha256 };
            masters.push(masterProof);
            for (const container of master.codec === 'av1' ? ['mp4', 'mkv', 'webm'] : ['mp4', 'mkv']) {
                const id = `${master.id}-${container}`;
                const output = join(directory, `${id}.${container}`);
                let probe = masterProbe;
                let identity = masterIdentity;
                let remux = { exactPixels: true, exactAudioPcm: true, master: true, frames: FRAMES, relativeTiming: true };
                if (container !== 'mp4') {
                    created.push(output);
                    await boundedMedia(ffmpeg, [...mediaPrelude, '-copyts', '-i', path, '-map', '0', '-map_metadata', '0', '-c', 'copy', '-disposition:a:0', '0', '-disposition:a:1', 'default', '-avoid_negative_ts', 'disabled', '-fs', String(FILE_LIMIT), '-n', output]);
                    identity = await fileIdentity(output, FILE_LIMIT);
                    probe = await probeSource(ffprobe, output);
                    validateProbe(probe, master);
                    validateHdr(probe, master, reference);
                    const metadata = compareMetadata(masterProbe, probe);
                    const frames = compareFramehash(decoded, await decodedVideo(ffmpeg, output, reference.pixelFormat));
                    const pcm = [];
                    for (const [index, hz] of [[1, 440], [2, 880]]) {
                        const actual = await decodedAudio(ffmpeg, output, index, hz);
                        if (actual.sha256 !== audio[index - 1].sha256 || actual.bytes !== audio[index - 1].bytes) throw new Error('Remux changed decoded Opus PCM or padding.');
                        pcm.push(actual);
                    }
                    remux = { ...frames, metadata, exactAudioPcm: true, audio: pcm, master: false };
                }
                const sourceConfiguration = await probeConfiguration(ffprobe, output, master.codec, container);
                if (sourceConfiguration.bitDepth !== master.bitDepth || sourceConfiguration.codecString.split('.').slice(1).join('.') !== configuration.codecString.split('.').slice(1).join('.')) throw new Error(`Actual container configuration changed its codec parameters: ${id}.`);
                if (container === 'mp4' && sourceConfiguration.configurationHex !== configuration.configurationHex) throw new Error(`Demuxed MP4 configuration differs from its actual sample-entry record: ${id}.`);
                const capability = capabilityFor(container, sourceConfiguration, probe, bitrate, tones[1].bitrate, master.dynamicRange);
                cases.push({ id, master: master.id, container, codec: master.codec, bitDepth: master.bitDepth, dynamicRange: master.dynamicRange, path: output, filename: basename(output), ...identity, videoIndex: 0, audioIndex: 2, audioHz: 880, width: WIDTH, height: HEIGHT, framerate: FRAME_RATE, bitrate, bitrateSource: actualBitrate > 0 ? 'master-video-stream' : 'master-container-ceiling', contentType: capability.video.contentType, audio: { ...capability.audio, channels: 1 }, probe, capability, configuration: sourceConfiguration, remux });
            }
        }
        if (cases.length !== 14) throw new Error('The reference corpus is incomplete.');
        for (const item of cases) if ((await fileIdentity(item.path, FILE_LIMIT)).sha256 !== item.sha256) throw new Error('Original corpus source changed during qualification.');
        const manifest = { version: 1, width: WIDTH, height: HEIGHT, framerate: FRAME_RATE, seconds: 8, frames: FRAMES, fileLimit: FILE_LIMIT, cases, masters, audioSources: tones.map(({ path, ...evidence }) => evidence), tools, proof: { files: 14, masters: 6, sequential: true, remuxVideoEncodeCount: 0, exactRemuxPixels: true, exactRemuxAudioPcm: true, neutralGrayscaleOnly: true, toleranceFixedBeforeExecution: true } };
        const json = `${JSON.stringify(manifest)}\n`;
        if (Buffer.byteLength(json) > 1048576) throw new Error('Corpus manifest exceeded its bound.');
        const manifestFile = await open(manifestPath, 'wx', 0o600);
        manifestCreated = true;
        try { await manifestFile.writeFile(json); } finally { await manifestFile.close(); }
        completed = true;
        return manifest;
    } finally {
        if (dirname(work) !== directory || !basename(work).startsWith('.format-work-')) throw new Error('Corpus work cleanup escaped its owned path.');
        await rm(work, { recursive: true, force: true });
        if (!completed) {
            if (manifestCreated) await unlink(manifestPath);
            for (const path of created.reverse()) { try { await unlink(path); } catch (error) { if (error.code !== 'ENOENT') throw error; } }
        }
    }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const [flag, directory, ...extra] = process.argv.slice(2);
    if (flag !== '--directory' || !directory || extra.length) { console.error('Use --directory with the exact empty owned corpus path.'); process.exitCode = 1; }
    else generateFormatCorpus({ directory }).then((manifest) => console.log(`DUSKCUE_FORMAT_CORPUS=${JSON.stringify({ version: manifest.version, files: manifest.cases.length, masters: manifest.masters.length, passed: true })}`)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
