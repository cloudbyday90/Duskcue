// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, statSync, writeFileSync } from 'node:fs';

const directory = '/src/.cache/tonight-server-ffmpeg';
const source = `${directory}/source.mkv`;

function checked(program, args) {
    const result = spawnSync(program, args, { encoding: 'utf8', timeout: 30000, maxBuffer: 1048576, stdio: ['ignore', 'pipe', 'pipe'] });
    if (result.error || result.status !== 0 || result.signal) throw new Error(`Bounded recovery fixture ${program} failed: ${result.stderr || result.error?.message || result.signal}`);
    return result.stdout;
}

for (const [name, text] of [['first', 'UNSELECTED CAPTION'], ['second', 'DUSKCUE SELECTED CAPTION']]) writeFileSync(`${directory}/${name}.srt`, `1\n00:00:01,000 --> 00:00:07,000\n${text}\n\n`, { flag: 'wx' });
checked('/usr/bin/ffmpeg', [
    '-hide_banner', '-loglevel', 'error', '-nostdin',
    '-f', 'lavfi', '-i', 'sine=frequency=220:sample_rate=48000',
    '-f', 'lavfi', '-i', 'color=c=red:size=320x180:rate=24',
    '-f', 'lavfi', '-i', 'sine=frequency=880:sample_rate=48000',
    '-i', `${directory}/first.srt`, '-i', `${directory}/second.srt`,
    '-map', '0:a:0', '-map', '1:v:0', '-map', '2:a:0', '-map', '3:s:0', '-map', '4:s:0',
    '-t', '8', '-fs', '16777216', '-c:v', 'libx264', '-preset', 'ultrafast', '-g', '48', '-keyint_min', '48', '-sc_threshold', '0', '-threads', '1',
    '-c:a', 'aac', '-c:s', 'srt', '-disposition:a:0', '0', '-disposition:a:1', 'default+visual_impaired',
    '-metadata:s:a:0', 'language=eng', '-metadata:s:a:1', 'language=fra', '-disposition:s:0', '0', '-disposition:s:1', 'hearing_impaired', '-metadata:s:s:1', 'language=fra', '-n', source,
]);
const probe = JSON.parse(checked('/usr/bin/ffprobe', ['-v', 'error', '-show_streams', '-show_format', '-of', 'json', source]));
const bytes = statSync(source).size;
const seconds = Number(probe.format?.duration);
if (bytes <= 0 || bytes > 16777216 || !Number.isFinite(seconds) || Math.abs(seconds - 8) > 0.1 || probe.streams?.length !== 5
    || probe.streams[0].codec_type !== 'audio' || probe.streams[1].codec_name !== 'h264' || probe.streams[1].width !== 320 || probe.streams[1].height !== 180
    || probe.streams[2].codec_name !== 'aac' || probe.streams[2].index !== 2 || probe.streams[2].disposition?.default !== 1) throw new Error('Recovery source stream layout or bounded duration changed.');
const fixture = { seconds, bytes, sha256: createHash('sha256').update(readFileSync(source)).digest('hex'), source_audio_stream_index: 2, ffmpegSha256: createHash('sha256').update(readFileSync('/usr/bin/ffmpeg')).digest('hex'), ffprobeSha256: createHash('sha256').update(readFileSync('/usr/bin/ffprobe')).digest('hex'), testExecutableSha256: createHash('sha256').update(readFileSync('/qualification/bin/playback-stop-contract')).digest('hex') };
writeFileSync('/qualification/fixture.json', `${JSON.stringify(fixture)}\n`, { flag: 'wx' });
console.log(JSON.stringify(fixture));
