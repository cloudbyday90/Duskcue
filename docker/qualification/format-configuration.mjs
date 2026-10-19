// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { createHash } from 'node:crypto';
import { codecConfiguration } from '../../scripts/qualification/format-codecs.mjs';
import { boundedMedia } from './format-process.mjs';

export function demuxedConfiguration(probe, codec, container) {
    if (!['hevc', 'av1'].includes(codec) || !['mp4', 'mkv', 'webm'].includes(container) || container === 'webm' && codec !== 'av1') throw new Error('Unsupported corpus configuration context.');
    if (!Array.isArray(probe?.streams) || probe.streams.length !== 1) throw new Error('Expected one selected source video configuration.');
    const stream = probe.streams[0];
    if (stream.index !== 0 || stream.codec_type !== 'video' || stream.codec_name !== codec || !Number.isInteger(stream.level)
        || !Number.isInteger(stream.extradata_size) || stream.extradata_size < 1 || stream.extradata_size > 65536
        || typeof stream.extradata !== 'string' || stream.extradata.length > 4 * 65536 / 3 + 2048) throw new Error('Invalid selected source configuration metadata.');
    const encoded = stream.extradata.replace(/[\t\n\v\f\r ]/g, '');
    if (encoded.length > 4 * 65536 / 3 + 4 || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(encoded)) throw new Error('Invalid source configuration Base64.');
    const bytes = Buffer.from(encoded, 'base64');
    if (bytes.length !== stream.extradata_size || bytes.toString('base64') !== encoded) throw new Error('Source configuration byte count or encoding changed.');
    const sampleEntry = container === 'mp4' ? stream.codec_tag_string : null;
    if (container === 'mp4' && !(codec === 'hevc' ? ['hvc1', 'hev1'] : ['av01']).includes(sampleEntry)) throw new Error('Source MP4 sample entry is absent or incompatible.');
    const configuration = codecConfiguration(codec, bytes, sampleEntry ?? (codec === 'hevc' ? 'hev1' : 'av01'));
    if (configuration.level !== stream.level) throw new Error('Source configuration level differs from its decoded stream.');
    return { ...configuration, sampleEntry, configurationSha256: createHash('sha256').update(bytes).digest('hex'), configurationSource: 'ffprobe-demuxed-extradata', streamIndex: stream.index };
}

export async function probeConfiguration(ffprobe, path, codec, container) {
    const output = await boundedMedia(ffprobe, ['-v', 'error', '-analyzeduration', '5000000', '-probesize', '5000000', '-select_streams', 'V', '-show_streams', '-show_data', '-data_dump_format', 'base64', '-show_entries', 'stream=index,codec_type,codec_name,codec_tag_string,level,extradata,extradata_size', '-of', 'json', '-i', path], { timeoutMs: 5000, outputLimit: 196608 });
    return demuxedConfiguration(JSON.parse(output.toString('utf8')), codec, container);
}
