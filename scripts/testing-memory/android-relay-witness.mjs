// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { request as httpRequest } from 'node:http';
import { once } from 'node:events';
import { writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { boundedBody, fixedOrigin } from './android-fault-policy.mjs';
import { runBounded } from '../qualification/commands.mjs';
import { hashFile } from '../qualification/source.mjs';

const uuid = '[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}';
const media = new RegExp(`^/api/v1/playback-commands/${uuid}/outputs/${uuid}/hls/(?:manifest\\.m3u8|[A-Za-z0-9_-]{1,64}/(?:index\\.m3u8|init\\.mp4|seg_[0-9]{1,50}\\.m4s))$`);

export function createRelayWitness(fixture, directory) {
    const origin = fixedOrigin(fixture.origin), aliases = new Map(), probes = [], pending = new Set();
    const stats = { rootMedia: 0, master: 0, variant: 0 };
    let master = false, busy = false;
    async function fetch(path, headers, signal) {
        if (!media.test(path)) throw new Error('Output probe left its exact media route set.');
        const request = httpRequest({ hostname: origin.hostname, port: origin.port, path, agent: false, signal,
            headers: { authorization: `Bearer ${fixture.token}`, 'x-session-context': headers['x-session-context'], 'accept-encoding': 'identity' }, maxHeaderSize: 8192 });
        const closed = new Promise((resolve) => request.once('close', resolve));
        pending.add(closed);
        try {
            const received = once(request, 'response'); request.end();
            const [response] = await received;
            if (response.statusCode !== 200 || response.headers['content-encoding']) throw new Error('Output probe refused status/coding.');
            return await boundedBody(response, 8388608);
        } finally { request.destroy(); await closed; pending.delete(closed); }
    }
    return {
        probes, stats,
        path(path) { const alias = aliases.get(path); if (media.test(path) && path.endsWith('/index.m3u8')) stats.variant++; return alias ?? path; },
        manifest(path, bytes) {
            if (!path.endsWith('/manifest.m3u8') || aliases.has(path)) return bytes;
            if (!master) { stats.rootMedia++; return bytes; }
            const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
            if (text.includes('#EXT-X-STREAM-INF:')) { stats.master++; return bytes; }
            const init = text.match(/^#EXT-X-MAP:URI="([^"]+)"$/m)?.[1];
            if (!init || !media.test(init) || !init.startsWith(path.slice(0, -'manifest.m3u8'.length))) throw new Error('Master witness requires current generated initialization.');
            const alias = init.replace(/init\.mp4$/, 'index.m3u8');
            if (aliases.size >= 16 && !aliases.has(alias)) throw new Error('Master witness map exhausted.');
            aliases.set(alias, path);
            stats.master++;
            return Buffer.from(`#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-STREAM-INF:BANDWIDTH=12000000,CODECS="avc1.64001f,mp4a.40.2"\n${alias}\n`);
        },
        async probe(body, headers, signal) {
            if (busy || probes.length >= 3 || Object.keys(body).sort().join(',') !== 'height,output,start'
                || !new RegExp(`^${uuid}$`).test(body.start) || !new RegExp(`^${uuid}$`).test(body.output)
                || body.height !== [1080, 720, 480][probes.length]) throw new Error('Output probe attribution differs.');
            busy = true;
            try {
                const prefix = `/api/v1/playback-commands/${body.start}/outputs/${body.output}/hls/`;
                let text = (await fetch(`${prefix}manifest.m3u8`, headers, signal)).toString('utf8');
                if (text.includes('#EXT-X-STREAM-INF:')) {
                    const variant = text.split('\n').find((line) => line.startsWith(prefix) && line.endsWith('/index.m3u8'));
                    if (!variant) throw new Error('Generated probe variant missing.');
                    text = (await fetch(variant, headers, signal)).toString('utf8');
                }
                const init = text.match(/^#EXT-X-MAP:URI="([^"]+)"$/m)?.[1];
                const paths = [init, ...text.split('\n').filter((line) => line.startsWith(prefix) && line.endsWith('.m4s')).slice(0, 8)];
                if (!init || !init.startsWith(prefix) || paths.length < 2) throw new Error('Generated probe output missing.');
                const parts = []; let size = 0;
                for (const path of paths) { const bytes = await fetch(path, headers, signal); size += bytes.length;
                    if (size > 8388608) throw new Error('Generated output probe exceeded its file bound.'); parts.push(bytes); }
                const path = join(directory, `generated-${body.height}.mp4`);
                await writeFile(path, Buffer.concat(parts, size), { flag: 'wx' });
                const tool = join(dirname(process.env.DUSKCUE_TEST_FFMPEG_EXE), 'ffprobe.exe');
                const observed = await runBounded(tool, ['-v', 'error', '-show_streams', '-of', 'json', path],
                    { timeoutMs: 10000, bytes: 65536, abortSignal: signal });
                const streams = JSON.parse(observed.stdout).streams;
                const video = streams?.find((s) => s.codec_type === 'video'), audio = streams?.find((s) => s.codec_type === 'audio');
                if (observed.code !== 0 || observed.signal || video?.height !== body.height || video.codec_name !== 'h264'
                    || video.color_primaries !== 'bt709' || video.color_transfer !== 'bt709' || video.color_space !== 'bt709'
                    || audio?.codec_name !== 'aac' || audio.channels !== 2) throw new Error('Actual generated output format differs.');
                const result = { height: video.height, width: video.width, channels: audio.channels, bt709: true,
                    generated_output: true, bytes: size, sha256: await hashFile(path), ffprobeSha256: await hashFile(tool), output: body.output, start: body.start };
                probes.push(result); master = true;
                await writeFile(join(directory, 'generated-output-probes.json'), JSON.stringify(probes, null, 2));
                return result;
            } finally { busy = false; }
        },
        async close() { await Promise.all([...pending]); if (busy || pending.size) throw new Error('Output probe cleanup is not settled.'); },
    };
}
