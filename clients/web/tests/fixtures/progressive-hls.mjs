/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { lstat, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createMediaResponder } from './media-response.mjs';

export function parseCompletedHlsPlaylist(content) {
    if (typeof content !== 'string' || Buffer.byteLength(content) > 65536) throw new Error('Fixture playlist exceeds its bound.');
    const lines = content.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
    if (lines[0] !== '#EXTM3U' || lines.at(-1) !== '#EXT-X-ENDLIST') throw new Error('A completed encoded playlist is required.');
    const headers = [];
    const segments = [];
    let pending = null;
    for (const line of lines.slice(1, -1)) {
        if (line === '#EXT-X-PLAYLIST-TYPE:VOD') continue;
        if (line.startsWith('#EXTINF:')) {
            if (pending !== null) throw new Error('Segment duration has no asset.');
            const match = /^#EXTINF:(\d+(?:\.\d+)?),$/.exec(line);
            const duration = Number(match?.[1]);
            if (!match || !Number.isFinite(duration) || duration <= 0 || duration > 60) throw new Error('Unsupported segment duration.');
            pending = { duration, tag: line };
        } else if (line.startsWith('#')) {
            if (pending !== null || segments.length || !/^#EXT-X-(?:VERSION:\d+|TARGETDURATION:\d+|MEDIA-SEQUENCE:0|INDEPENDENT-SEGMENTS)$/.test(line)) throw new Error('Unsupported encoded fixture playlist tag.');
            headers.push(line);
        } else {
            if (!pending || !/^segment-\d{3}\.ts$/.test(line) || segments.some((segment) => segment.name === line)) throw new Error('Unsupported encoded fixture segment.');
            segments.push(Object.freeze({ ...pending, name: line }));
            pending = null;
        }
    }
    if (pending !== null || !segments.length || segments.length > 256 || !headers.some((line) => /^#EXT-X-TARGETDURATION:[1-9]\d*$/.test(line))) throw new Error('Incomplete encoded fixture playlist.');
    const totalDuration = segments.reduce((sum, segment) => sum + segment.duration, 0);
    const render = (count, complete = false) => {
        if (!Number.isSafeInteger(count) || count < 1 || count > segments.length || (complete && count !== segments.length)) throw new Error('Invalid progressive publication boundary.');
        return ['#EXTM3U', ...headers, '#EXT-X-PLAYLIST-TYPE:EVENT', ...segments.slice(0, count).flatMap((segment) => [segment.tag, segment.name]), ...(complete ? ['#EXT-X-ENDLIST'] : []), ''].join('\n');
    };
    return Object.freeze({ segments: Object.freeze(segments), totalDuration, render });
}

export async function createProgressiveHlsResponder(media, { clip = 'standard', initialSegments = 3 } = {}) {
    if (!['standard', 'ending'].includes(clip)) throw new Error('Unknown encoded fixture clip.');
    const playlistPath = join(media.directory, clip, 'index.m3u8');
    const playlistFile = await lstat(playlistPath);
    if (!playlistFile.isFile() || playlistFile.size > 65536) throw new Error('Encoded fixture playlist is not a bounded file.');
    const playlist = parseCompletedHlsPlaylist(await readFile(playlistPath, 'utf8'));
    let assetBytes = 0;
    for (const segment of playlist.segments) {
        const asset = await lstat(join(media.directory, clip, segment.name));
        if (!asset.isFile() || asset.size <= 0 || asset.size > 4 * 1024 ** 2) throw new Error('Encoded segment is not a bounded file.');
        assetBytes += asset.size;
    }
    if (assetBytes > 32 * 1024 ** 2) throw new Error('Encoded fixture assets exceed their combined bound.');
    let published = initialSegments;
    let complete = false;
    playlist.render(published);
    const requests = [];
    const serveAsset = createMediaResponder(media);
    const snapshot = () => ({ published, count: playlist.segments.length, duration: playlist.totalDuration, producedDuration: playlist.segments.slice(0, published).reduce((sum, segment) => sum + segment.duration, 0), complete, requests: requests.slice() });
    return {
        state: snapshot,
        publish(count) {
            playlist.render(count);
            if (complete || count < published) throw new Error('An EVENT playlist can only grow.');
            published = count;
        },
        complete() { published = playlist.segments.length; complete = true; },
        async serve(route, name, contentType) {
            if (requests.length >= 512) throw new Error('Progressive fixture request bound exhausted.');
            requests.push({ name, published, complete });
            if (name === `${clip}/index.m3u8`) return route.fulfill({ contentType: 'application/vnd.apple.mpegurl', headers: { 'cache-control': 'no-store' }, body: playlist.render(published, complete) });
            const segment = playlist.segments.findIndex((asset) => name === `${clip}/${asset.name}`);
            if (segment >= published) throw new Error('A decoder requested an unpublished segment.');
            return serveAsset(route, name, contentType);
        },
    };
}
