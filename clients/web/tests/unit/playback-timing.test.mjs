/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 */

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Hls from 'hls.js';
import { canCompleteSource, onDemandHlsTiming, sourcePlaybackDuration, sourceTimeline } from '../../src/lib/playback/timing.js';
import { createMediaSource } from '../../src/lib/playback/source.js';

function media() {
    return Object.assign(new EventTarget(), { src: '', currentTime: 0, error: null, pause() {}, load() {}, play() { return Promise.resolve(); }, removeAttribute() {}, canPlayType() { return 'probably'; } });
}

class FakeHls {
    static Events = { MANIFEST_PARSED: 'manifest', LEVEL_UPDATED: 'level', ERROR: 'error' };
    static ErrorTypes = { NETWORK_ERROR: 'network', MEDIA_ERROR: 'media' };
    static instances = [];
    static isSupported() { return true; }
    handlers = new Map();
    destroyed = false;
    constructor(config) { this.config = config; FakeHls.instances.push(this); }
    on(name, handler) { this.handlers.set(name, handler); }
    loadSource(url) { this.url = url; }
    attachMedia(video) { this.video = video; }
    destroy() { this.destroyed = true; this.handlers.clear(); }
}

test('known runtime survives pending HLS and every growing EVENT window', () => {
    const options = { runtimeSeconds: 600, streamOffsetMs: 120_000 };
    assert.equal(sourcePlaybackDuration({ ...options, source: sourceTimeline({ generation: 1, mode: 'hls-pending' }), mediaDurationSeconds: Infinity }), 600_000);
    for (const duration of [6, 12, 24, 42]) {
        const source = sourceTimeline({ generation: 1, mode: 'hls-mse', details: { type: 'EVENT', live: true, totalduration: duration, edge: duration } });
        assert.equal(sourcePlaybackDuration({ ...options, source, mediaDurationSeconds: duration }), 600_000);
        assert.equal(canCompleteSource(source), false);
    }
});

test('final EVENT and VOD duration maps their final source end to absolute media time', () => {
    for (const type of ['EVENT', 'VOD']) {
        const source = sourceTimeline({ generation: 1, mode: 'hls-mse', details: { type, live: false, totalduration: 480, edge: 480 } });
        assert.equal(sourcePlaybackDuration({ source, mediaDurationSeconds: 6, runtimeSeconds: 601, streamOffsetMs: 120_000 }), 600_000);
        assert.equal(canCompleteSource(source), true);
    }
    assert.equal(canCompleteSource(sourceTimeline({ generation: 2, mode: 'hls-mse', details: { type: 'VOD', live: true } })), false);
});

test('completed MP4 and natural native HLS completion remain available', () => {
    const direct = sourceTimeline({ generation: 1, mode: 'direct' });
    assert.equal(sourcePlaybackDuration({ source: direct, runtimeSeconds: 500, mediaDurationSeconds: 600 }), 600_000);
    assert.equal(canCompleteSource(direct), true);
    const native = sourceTimeline({ generation: 2, mode: 'native-hls' });
    assert.equal(native.complete, null);
    assert.equal(sourcePlaybackDuration({ source: native, runtimeSeconds: 600, mediaDurationSeconds: 6, streamOffsetMs: 120_000 }), 600_000);
    assert.equal(sourcePlaybackDuration({ source: native, runtimeSeconds: 600, mediaDurationSeconds: 481, streamOffsetMs: 120_000, naturalEnded: true }), 601_000);
    assert.equal(canCompleteSource(native), true);
});

test('unknown runtime uses only available finite media timing and invalid offsets never leak', () => {
    const source = sourceTimeline({ generation: 1, mode: 'hls-mse', details: { type: 'EVENT', live: true } });
    assert.equal(sourcePlaybackDuration({ source, runtimeSeconds: 0, mediaDurationSeconds: 6, streamOffsetMs: 120_000 }), 126_000);
    assert.equal(sourcePlaybackDuration({ source, runtimeSeconds: NaN, mediaDurationSeconds: Infinity, streamOffsetMs: Infinity }), 0);
    assert.equal(sourcePlaybackDuration({ source, runtimeSeconds: -1, mediaDurationSeconds: 6, streamOffsetMs: -1 }), 6000);
    assert.equal(canCompleteSource(sourceTimeline({ generation: 2 })), false);
});

test('locked hls.js accepts zero start and infinite on-demand sync latency through its actual validator', () => {
    assert.equal(Hls.version, '1.6.16');
    assert.throws(() => new Hls({ liveSyncDuration: Infinity, liveSyncDurationCount: 3 }), /don't mix/);
    const engine = new Hls({ ...onDemandHlsTiming, enableWorker: false, lowLatencyMode: false });
    try {
        assert.equal(engine.config.startPosition, 0);
        assert.equal(engine.config.liveSyncDuration, Infinity);
        assert.equal(engine.config.liveMaxLatencyDurationCount, Infinity);
        assert.equal(engine.config.lowLatencyMode, false);
    } finally { engine.destroy(); }
});

test('source snapshots stay immutable and obsolete engine updates cannot finalize a replacement', async () => {
    FakeHls.instances = [];
    const changes = [];
    const source = createMediaSource({ video: media(), loadHls: async () => FakeHls, requiresHeaders: () => false, onTimelineChange: (value) => changes.push(value) });
    await source.attach('/api/v1/transcode/one/manifest.m3u8');
    const first = FakeHls.instances[0];
    const updateFirst = first.handlers.get(FakeHls.Events.LEVEL_UPDATED);
    updateFirst('level', { details: { type: 'EVENT', live: true, totalduration: 6, edge: 6 } });
    const held = source.getTimeline();
    assert.equal(Object.isFrozen(held), true);
    assert.equal(Reflect.set(held, 'complete', true), false);
    await source.attach('/api/v1/transcode/two/manifest.m3u8');
    assert.equal(first.destroyed, true);
    const before = changes.length;
    updateFirst('level', { details: { type: 'EVENT', live: false, totalduration: 600, edge: 600 } });
    assert.equal(changes.length, before);
    assert.equal(source.getTimeline().complete, null);
    assert.equal(held.producedDurationMs, 6000);
    assert.equal(held.complete, false);
    const second = FakeHls.instances[1];
    const updateSecond = second.handlers.get(FakeHls.Events.LEVEL_UPDATED);
    updateSecond('level', { details: { type: 'VOD', live: false, totalduration: 20, edge: 20 } });
    assert.equal(canCompleteSource(source.getTimeline()), true);
    source.dispose();
    const disposedChanges = changes.length;
    updateSecond('level', { details: { type: 'VOD', live: false, totalduration: 90, edge: 90 } });
    assert.equal(changes.length, disposedChanges);
    assert.equal(source.getTimeline().mode, 'detached');
    assert.equal(second.destroyed, true);
});

test('native fallback exposes no observed final metadata and late HLS imports cannot publish after disposal', async () => {
    const native = createMediaSource({ video: media(), loadHls: async () => ({ isSupported: () => false }), requiresHeaders: () => false });
    await native.attach('/api/v1/transcode/native/manifest.m3u8');
    assert.equal(native.getTimeline().mode, 'native-hls');
    assert.equal(native.getTimeline().complete, null);
    native.dispose();
    let resolve;
    const pending = new Promise((accept) => { resolve = accept; });
    const changes = [];
    const source = createMediaSource({ video: media(), loadHls: () => pending, onTimelineChange: (value) => changes.push(value) });
    const attaching = source.attach('/api/v1/transcode/pending/manifest.m3u8');
    source.dispose();
    const count = changes.length;
    resolve(FakeHls);
    await attaching;
    assert.equal(changes.length, count);
    assert.equal(source.getTimeline().mode, 'detached');
});
