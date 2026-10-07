/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseCompletedHlsPlaylist } from './progressive-hls.mjs';

const source = '#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXTINF:2.000000,\nsegment-000.ts\n#EXTINF:2.000000,\nsegment-001.ts\n#EXTINF:1.500000,\nsegment-002.ts\n#EXT-X-ENDLIST\n';

test('published EVENT prefixes preserve actual durations and stable earlier segments', () => {
    const playlist = parseCompletedHlsPlaylist(source);
    assert.equal(playlist.totalDuration, 5.5);
    const first = playlist.render(1);
    const grown = playlist.render(2);
    assert.match(first, /#EXT-X-PLAYLIST-TYPE:EVENT/);
    assert.doesNotMatch(first, /ENDLIST|segment-001/);
    assert.ok(grown.startsWith(first.trimEnd()));
    assert.match(grown, /#EXTINF:2.000000,\nsegment-001.ts/);
    const final = playlist.render(3, true);
    assert.match(final, /segment-002.ts\n#EXT-X-ENDLIST\n$/);
});

test('finality cannot be declared before all encoded segments are published', () => {
    const playlist = parseCompletedHlsPlaylist(source);
    for (const count of [0, -1, 1.5, 4, NaN]) assert.throws(() => playlist.render(count));
    assert.throws(() => playlist.render(1, true));
});

test('invalid, external, traversal and unsupported fixture playlists are refused', () => {
    for (const body of [
        source.replace('#EXT-X-ENDLIST\n', ''),
        source.replace('segment-000.ts', '../segment-000.ts'),
        source.replace('segment-000.ts', 'https://foreign.test/segment-000.ts'),
        source.replace('#EXTINF:2.000000,', '#EXTINF:0,'),
        source.replace('segment-001.ts', 'segment-000.ts'),
        source.replace('#EXT-X-INDEPENDENT-SEGMENTS', '#EXT-X-KEY:METHOD=AES-128,URI="secret.key"'),
        source.replace('#EXT-X-INDEPENDENT-SEGMENTS', '#EXT-X-MAP:URI="init.mp4"'),
        `${'x'.repeat(65537)}${source}`,
    ]) assert.throws(() => parseCompletedHlsPlaylist(body));
});
