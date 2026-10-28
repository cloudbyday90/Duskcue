// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { readFile, stat, writeFile } from 'node:fs/promises';
import { runBounded } from './commands.mjs';
import { hashFile } from './source.mjs';

export async function authorVodDamage(source, destination, ffprobe) {
    const metadata = await stat(source);
    if (!metadata.isFile() || metadata.size < 1 || metadata.size > 8388608) throw new Error('Bounded authored MP4 required.');
    const originalHash = await hashFile(source);
    const bytes = await readFile(source);
    const probe = await runBounded(ffprobe, ['-v', 'error', '-select_streams', 'v:0', '-show_packets', '-show_streams',
        '-show_entries', 'stream=index,codec_name:packet=stream_index,pts_time,pos,size', '-of', 'json', source], { timeoutMs: 10000, bytes: 1048576 });
    if (probe.code !== 0 || probe.signal) throw new Error('Owned damage fixture probe failed.');
    const data = JSON.parse(probe.stdout);
    if (data.streams?.length !== 1 || data.streams[0].index !== 1 || data.streams[0].codec_name !== 'h264' || data.packets?.length !== 192) throw new Error('Expected the authored audio-first AVC fixture.');
    let samples = 0;
    for (const packet of data.packets) {
        const pts = Number(packet.pts_time);
        if (!Number.isFinite(pts)) throw new Error('Invalid authored packet time.');
        if (pts < 2 || pts >= 4) continue;
        let offset = Number(packet.pos);
        const length = Number(packet.size);
        if (!Number.isSafeInteger(offset) || !Number.isSafeInteger(length) || offset < 0 || length < 1 || offset + length > bytes.length) throw new Error('Invalid authored packet boundary.');
        const end = offset + length;
        let nals = 0;
        let changed = 0;
        while (offset < end) {
            if (end - offset < 4 || ++nals > 64) throw new Error('Invalid authored AVC sample.');
            const size = bytes.readUInt32BE(offset);
            const header = offset + 4;
            const next = header + size;
            if (size < 1 || next > end || bytes[header] & 0x80 || (bytes[header] & 31) < 1 || (bytes[header] & 31) > 23) throw new Error('Invalid authored AVC NAL.');
            if ([1, 5].includes(bytes[header] & 31)) { if (size < 2) throw new Error('Empty VCL sample.'); bytes.fill(255, header + 1, next); changed++; }
            offset = next;
        }
        if (!changed) throw new Error('Damage did not affect a VCL sample.');
        samples++;
    }
    if (samples !== 48) throw new Error('Expected exactly 48 damaged frames.');
    await writeFile(destination, bytes, { flag: 'wx' });
    if (await hashFile(source) !== originalHash) throw new Error('Original source changed during damage authoring.');
    return { path: destination, sha256: await hashFile(destination), damagedSamples: samples, unchangedOriginalSha256: originalHash };
}
