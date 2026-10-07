import { open } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { join, resolve } from 'node:path';
import { test as base, expect } from './playback-fixtures';
import { byteRangeResponse } from '../fixtures/media-response.mjs';

const directory = process.env.DUSKCUE_TEST_CAPTION_HLS_DIR || null;

async function readBounded(path: string, limit: number) {
    const file = await open(path, 'r');
    try {
        const metadata = await file.stat();
        if (!metadata.isFile() || !Number.isSafeInteger(metadata.size) || metadata.size < 1 || metadata.size > limit) throw new Error(`Caption fixture must be a regular file between 1 and ${limit} bytes: ${path}`);
        const bytes = Buffer.allocUnsafe(metadata.size);
        let offset = 0;
        while (offset < bytes.length) {
            const { bytesRead } = await file.read(bytes, offset, bytes.length - offset, offset);
            if (!bytesRead) throw new Error(`Caption fixture changed during its bounded read: ${path}`);
            offset += bytesRead;
        }
        if ((await file.read(Buffer.allocUnsafe(1), 0, 1, bytes.length)).bytesRead) throw new Error(`Caption fixture grew during its bounded read: ${path}`);
        return bytes;
    } finally { await file.close(); }
}

export const test = base.extend<{
    captionOptions: { endDuringCue?: boolean };
    captions: { directory: string; manifestHash: string; segments: string[]; hashes: Record<string, string>; cue: { text: string; startMs: number; endMs: number; language: string; streamIndex: number; hearingImpaired: boolean }; producer: string };
}>({
    captionOptions: [{}, { option: true }],
    captions: async ({ page, api, playback, captionOptions }, use) => {
        if (!directory) { test.skip(true, 'Real-caption qualification is pending: set DUSKCUE_TEST_CAPTION_HLS_DIR to the output of the admitted production-argument caption test.'); return; }
        const manifestBytes = await readBounded(join(directory, 'manifest.m3u8'), 64 * 1024);
        const manifest = manifestBytes.toString('utf8');
        const segments = manifest.split(/\r?\n/).filter((line) => /^seg_\d+\.m4s$/.test(line));
        if (segments.length !== 4 || !manifest.includes('#EXT-X-ENDLIST')) throw new Error('Caption qualification requires the retained four-segment production-argument HLS fixture.');
        const hashes: Record<string, string> = {};
        const media = new Map<string, Buffer>();
        for (const name of ['manifest.m3u8', 'init.mp4', ...segments]) {
            const bytes = name === 'manifest.m3u8' ? manifestBytes : await readBounded(join(directory, name), name === 'init.mp4' ? 1024 * 1024 : 4 * 1024 * 1024);
            media.set(name, bytes);
            hashes[`caption/${name}`] = createHash('sha256').update(bytes).digest('hex');
        }
        const source = resolve(directory, '..');
        let selectedCue = '';
        for (const name of ['source.mkv', 'first.srt', 'second.srt']) {
            const bytes = await readBounded(join(source, name), name.endsWith('.srt') ? 4 * 1024 : 16 * 1024 * 1024);
            hashes[name] = createHash('sha256').update(bytes).digest('hex');
            if (name === 'second.srt') selectedCue = bytes.toString('utf8').replaceAll('\r', '').trim();
        }
        if (selectedCue !== '1\n00:00:01,000 --> 00:00:07,000\nDUSKCUE SELECTED CAPTION') throw new Error('The selected caption cue does not match the production-argument test source.');
        let playlist = manifest;
        if (captionOptions.endDuringCue) {
            const lines = manifest.split(/\r?\n/);
            const last = lines.indexOf(segments[3]);
            if (!lines[last - 1]?.startsWith('#EXTINF:')) throw new Error('The retained caption playlist cannot be shortened safely.');
            lines.splice(last - 1, 2);
            playlist = lines.join('\n');
        }
        for (const item of api.mediaItems) {
            if (!['movie', 'episode'].includes(item.type)) continue;
            item.runtime_seconds = captionOptions.endDuringCue ? 6 : 8;
            for (const file of api.files[item.id]) {
                file.runtime_seconds = item.runtime_seconds;
                file.additional_streams.subtitles = [{ index: 4, language: 'fra', codec: 'subrip', disposition: { default: 0, hearing_impaired: 1 } }];
            }
        }
        for (const saved of Object.values(api.preferencesByProfile)) Object.assign(saved.viewing_preferences, { subtitle_mode: 'always', subtitle_language: 'fr', prefer_sdh: true });
        await page.route(/\/api\/v1\/transcode\/[^/]+\/(manifest\.m3u8|init\.mp4|seg_\d+\.m4s)$/, async (route) => {
            const name = new URL(route.request().url()).pathname.split('/').at(-1)!;
            if (name === 'manifest.m3u8') return route.fulfill({ contentType: 'application/vnd.apple.mpegurl', body: playlist });
            const bytes = media.get(name);
            if (!bytes) return route.fulfill({ status: 404 });
            return route.fulfill({ ...byteRangeResponse(bytes, route.request().headers().range), contentType: name.endsWith('.m4s') ? 'video/iso.segment' : 'video/mp4' });
        });
        await use({ directory, manifestHash: hashes['caption/manifest.m3u8'], segments: captionOptions.endDuringCue ? segments.slice(0, 3) : segments, hashes, cue: { text: 'DUSKCUE SELECTED CAPTION', startMs: 1000, endMs: 7000, language: 'fra', streamIndex: 4, hearingImpaired: true }, producer: 'services::transcoding::arguments::tests::real_audio_first_default_description_and_selected_srt_decode_through_production_arguments' });
    },
});

export { expect };
