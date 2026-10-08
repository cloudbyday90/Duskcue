/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import test from 'node:test';
import assert from 'node:assert/strict';
import { createPlaybackBackground } from '../../src/lib/playback/background.js';
import { createAutoplayController } from '../../src/lib/playback/autoplay.js';

const item = { id: 'episode-one', type: 'episode' };
const next = { id: 'episode-two', type: 'episode' };
const context = { item, profileId: 'alex', autoplayEnabled: true, preferencesReady: true };
const flush = () => new Promise((resolve) => setImmediate(resolve));

function deferred() {
    let resolve;
    let reject;
    const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
    return { promise, resolve, reject };
}
function timing() {
    let now = 0;
    let identifier = 0;
    const timers = new Map();
    return {
        now: () => now,
        setTimeout(callback, delay) { const id = ++identifier; timers.set(id, { callback, time: now + delay }); return id; },
        clearTimeout: (id) => timers.delete(id),
        get size() { return timers.size; },
        async advance(delay) {
            await flush();
            const target = now + delay;
            for (;;) {
                const entry = [...timers].filter(([, timer]) => timer.time <= target).sort((a, b) => a[1].time - b[1].time)[0];
                if (!entry) break;
                const [id, timer] = entry;
                now = timer.time;
                timers.delete(id);
                timer.callback();
                await flush();
            }
            now = target;
            await flush();
        },
    };
}
function nativeWindow() {
    const listeners = new Set();
    return {
        minimized: false, visible: true, reads: 0,
        isMinimized() { this.reads += 1; return Promise.resolve(this.minimized); },
        isVisible() { return Promise.resolve(this.visible); },
        async onFocusChanged(callback) { listeners.add(callback); return () => listeners.delete(callback); },
        async onResized(callback) { listeners.add(callback); return () => listeners.delete(callback); },
        changed() { for (const callback of listeners) callback(); },
        get listeners() { return listeners.size; },
    };
}
function harness({ native = nativeWindow(), loadNative = async () => native, document = Object.assign(new EventTarget(), { hidden: false }), clock = timing() } = {}) {
    const changes = [];
    const starts = [];
    let background;
    const autoplay = createAutoplayController({ clock, resolveNext: async () => ({ kind: 'available', episode: next }), onChange: (state) => background?.setActive(state.phase === 'countdown'),
        beforeAutomaticNext: (options) => background.canAdvance(options), onPlayNext: async (_next, options) => { starts.push(options); } });
    background = createPlaybackBackground({ document, loadNative, clock, onChange: (state) => { changes.push(state); autoplay.setPaused(state); } });
    return { background, autoplay, native, document, clock, changes, starts, dispose() { background.dispose(); autoplay.dispose(); } };
}

test('native minimization or native hiding pauses a visible document and resumes exact remaining time', async () => {
    for (const reason of ['minimized', 'hidden']) {
        const h = harness();
        await h.background.ready;
        await h.autoplay.ended(context);
        await h.clock.advance(2750);
        h.native.minimized = reason === 'minimized';
        h.native.visible = reason !== 'hidden';
        h.native.changed();
        await flush();
        assert.equal(h.background.getState().documentHidden, false);
        assert.equal(h.background.getState().nativeBackground, true);
        assert.equal(h.autoplay.getState().remainingMs, 7250);
        await h.clock.advance(20_000);
        assert.equal(h.starts.length, 0);
        assert.equal(h.autoplay.getState().remainingMs, 7250);
        h.native.minimized = false;
        h.native.visible = true;
        h.native.changed();
        await flush();
        await h.clock.advance(7249);
        assert.equal(h.starts.length, 0);
        await h.clock.advance(1);
        assert.equal(h.starts.length, 1);
        assert.equal(h.starts[0].mode, 'automatic');
        h.dispose();
        assert.equal(h.clock.size, 0);
        assert.equal(h.native.listeners, 0);
    }
});

test('document hiding remains independent from clearing a native background reason', async () => {
    const h = harness();
    await h.background.ready;
    h.document.hidden = true;
    h.document.dispatchEvent(new Event('visibilitychange'));
    h.native.minimized = true;
    h.native.changed();
    await flush();
    await h.autoplay.ended(context);
    h.native.minimized = false;
    h.native.changed();
    await flush();
    assert.equal(h.background.getState().nativeBackground, false);
    assert.equal(h.autoplay.getState().paused, true);
    await h.clock.advance(20_000);
    assert.equal(h.starts.length, 0);
    h.dispose();
});

test('an event invalidates an older foreground query and native reads never overlap', async () => {
    const native = nativeWindow();
    const first = deferred();
    native.isMinimized = function () { this.reads += 1; return this.reads === 1 ? first.promise : Promise.resolve(this.minimized); };
    const h = harness({ native });
    await flush();
    assert.equal(native.reads, 1);
    native.minimized = true;
    native.changed();
    native.changed();
    await h.clock.advance(500);
    assert.equal(native.reads, 1);
    const count = h.changes.length;
    first.resolve(false);
    await h.background.ready;
    await flush();
    assert.equal(native.reads, 2);
    assert.ok(h.changes.slice(count).every((state) => state.nativeBackground));
    assert.equal(h.background.getState().nativeMinimized, true);
    h.dispose();
});

test('a stalled native read fails closed without creating overlapping polls, and a late result cannot clear it', async () => {
    const native = nativeWindow();
    const first = deferred();
    native.isMinimized = function () { this.reads += 1; return this.reads === 1 ? first.promise : Promise.resolve(this.minimized); };
    const h = harness({ native });
    await flush();
    await h.clock.advance(1500);
    await h.background.ready;
    await h.autoplay.ended(context);
    await h.clock.advance(20_000);
    assert.equal(native.reads, 1);
    assert.equal(h.background.getState().nativeKnown, false);
    assert.equal(h.starts.length, 0);
    native.minimized = true;
    first.resolve(false);
    await flush();
    assert.equal(native.reads, 2);
    assert.equal(h.background.getState().nativeBackground, true);
    h.dispose();
});

test('failed or invalid native reads deny automatic advancement while manual next stays untimed', async () => {
    for (const read of [async () => { throw new Error('native unavailable'); }, async () => null]) {
        const native = nativeWindow();
        native.isMinimized = read;
        const h = harness({ native });
        await h.background.ready;
        await h.autoplay.ended(context);
        await h.clock.advance(20_000);
        assert.equal(h.background.getState().nativeKnown, false);
        assert.equal(h.starts.length, 0);
        assert.equal(await h.autoplay.playNext(), true);
        assert.equal(h.starts[0].mode, 'manual');
        h.dispose();
    }
});

test('one failed native read cannot start another poll while its paired read is still pending', async () => {
    const native = nativeWindow();
    const visible = deferred();
    native.isMinimized = function () { this.reads += 1; return this.reads === 1 ? Promise.reject(new Error('minimized read failed')) : Promise.resolve(false); };
    native.isVisible = function () { return this.reads === 1 ? visible.promise : Promise.resolve(true); };
    const h = harness({ native });
    await flush();
    await h.clock.advance(20_000);
    await h.background.ready;
    assert.equal(native.reads, 1);
    assert.equal(h.background.getState().nativeKnown, false);
    visible.resolve(true);
    await flush();
    assert.equal(native.reads, 2);
    assert.equal(h.background.getState().nativeKnown, true);
    h.dispose();
});

test('automatic expiry takes a fresh native read rather than trusting the previous visible poll', async () => {
    const h = harness();
    await h.background.ready;
    await h.autoplay.ended(context);
    await h.clock.advance(9999);
    h.native.minimized = true;
    const reads = h.native.reads;
    await h.clock.advance(1);
    assert.ok(h.native.reads > reads);
    assert.equal(h.starts.length, 0);
    assert.equal(h.autoplay.getState().paused, true);
    h.dispose();
});

test('dispose releases timers and late subscriptions without publishing deferred native state', async () => {
    const native = nativeWindow();
    const read = deferred();
    const focus = deferred();
    const resize = deferred();
    let stopped = 0;
    native.isMinimized = () => read.promise;
    native.onFocusChanged = () => focus.promise;
    native.onResized = () => resize.promise;
    const h = harness({ native });
    await flush();
    h.dispose();
    const count = h.changes.length;
    assert.equal(h.clock.size, 0);
    focus.resolve(() => { stopped += 1; });
    resize.resolve(() => { stopped += 1; });
    read.resolve(false);
    await h.background.ready;
    await flush();
    assert.equal(stopped, 2);
    assert.equal(h.changes.length, count);
    assert.equal(await h.background.canAdvance(), false);
});

test('a native import finishing after disposal cannot subscribe, poll or publish', async () => {
    const load = deferred();
    const h = harness({ loadNative: () => load.promise });
    h.dispose();
    const count = h.changes.length;
    load.resolve(h.native);
    await h.background.ready;
    assert.equal(h.native.reads, 0);
    assert.equal(h.native.listeners, 0);
    assert.equal(h.changes.length, count);
    assert.equal(h.clock.size, 0);
});

test('a delayed automatic gate from the old profile cannot advance the new profile', async () => {
    const clock = timing();
    const gate = deferred();
    const checks = [];
    const starts = [];
    const autoplay = createAutoplayController({ clock, resolveNext: async () => ({ kind: 'available', episode: next }),
        beforeAutomaticNext: (options) => { checks.push(options); return checks.length === 1 ? gate.promise : Promise.resolve(true); },
        onPlayNext: async (_next, options) => { starts.push(options); } });
    await autoplay.ended(context);
    await clock.advance(10_000);
    assert.equal(checks.length, 1);
    await autoplay.ended({ ...context, profileId: 'morgan' });
    assert.equal(checks[0].signal.aborted, true);
    gate.resolve(true);
    await flush();
    assert.equal(starts.length, 0);
    assert.equal(autoplay.getState().remainingMs, 10_000);
    await clock.advance(10_000);
    assert.equal(starts.length, 1);
    assert.equal(starts[0].profileId, 'morgan');
    autoplay.dispose();
});

test('manual next may proceed during a delayed automatic gate without starting twice', async () => {
    const clock = timing();
    const gate = deferred();
    const starts = [];
    const autoplay = createAutoplayController({ clock, resolveNext: async () => ({ kind: 'available', episode: next }), beforeAutomaticNext: () => gate.promise,
        onPlayNext: async (_next, options) => { starts.push(options); } });
    await autoplay.ended(context);
    await clock.advance(10_000);
    assert.equal(await autoplay.playNext(), true);
    gate.resolve(true);
    await flush();
    assert.equal(starts.length, 1);
    assert.equal(starts[0].mode, 'manual');
    autoplay.dispose();
});

test('separate Player instances preserve profile and native background isolation', async () => {
    const alex = harness();
    const morgan = harness();
    await Promise.all([alex.background.ready, morgan.background.ready]);
    await alex.autoplay.ended(context);
    await morgan.autoplay.ended({ ...context, profileId: 'morgan' });
    alex.native.minimized = true;
    alex.native.changed();
    await flush();
    await Promise.all([alex.clock.advance(20_000), morgan.clock.advance(20_000)]);
    assert.equal(alex.starts.length, 0);
    assert.equal(morgan.starts.length, 1);
    assert.equal(morgan.starts[0].profileId, 'morgan');
    alex.dispose();
    morgan.dispose();
});

test('browser-only playback uses actual document visibility without native polling', async () => {
    const h = harness({ loadNative: async () => null });
    await h.background.ready;
    await h.autoplay.ended(context);
    await h.clock.advance(10_000);
    assert.equal(h.starts.length, 1);
    assert.equal(h.native.reads, 0);
    assert.equal(h.clock.size, 0);
    h.dispose();
});

test('saved Off and Cancel leave no periodic native polls while events still refresh actual state', async () => {
    const h = harness();
    await h.background.ready;
    await h.autoplay.ended({ ...context, autoplayEnabled: false });
    const initial = h.native.reads;
    await h.clock.advance(20_000);
    assert.equal(h.native.reads, initial);
    assert.equal(h.clock.size, 0);
    h.native.changed();
    await flush();
    assert.equal(h.native.reads, initial + 1);
    h.autoplay.setPreference({ autoplayEnabled: true, preferencesReady: true });
    await flush();
    await h.clock.advance(1200);
    assert.ok(h.native.reads > initial + 1);
    h.autoplay.cancel();
    await flush();
    const cancelled = h.native.reads;
    await h.clock.advance(20_000);
    assert.equal(h.native.reads, cancelled);
    assert.equal(h.clock.size, 0);
    assert.equal(h.starts.length, 0);
    h.dispose();
});

test('arming refreshes native state and a paused countdown keeps polling until a real restore is observed', async () => {
    const h = harness();
    await h.background.ready;
    h.native.minimized = true;
    await h.autoplay.ended(context);
    await flush();
    assert.equal(h.autoplay.getState().paused, true);
    await h.clock.advance(5000);
    assert.equal(h.autoplay.getState().remainingMs, 10_000);
    assert.equal(h.starts.length, 0);
    h.native.minimized = false;
    await h.clock.advance(400);
    assert.equal(h.autoplay.getState().paused, false);
    assert.ok(h.autoplay.getState().remainingMs > 9500);
    await h.clock.advance(10_000);
    assert.equal(h.starts.length, 1);
    const finished = h.native.reads;
    await h.clock.advance(5000);
    assert.equal(h.native.reads, finished);
    assert.equal(h.clock.size, 0);
    h.dispose();
});

test('reset or a preference change disarms polling without reviving a delayed read', async () => {
    for (const disarm of [(autoplay) => autoplay.reset(), (autoplay) => autoplay.setPreference({ autoplayEnabled: false, preferencesReady: true })]) {
        const h = harness();
        await h.background.ready;
        await h.autoplay.ended(context);
        await flush();
        const pending = deferred();
        h.native.isMinimized = function () { this.reads += 1; return pending.promise; };
        await h.clock.advance(400);
        disarm(h.autoplay);
        const reads = h.native.reads;
        pending.resolve(false);
        await flush();
        await h.clock.advance(20_000);
        assert.equal(h.native.reads, reads);
        assert.equal(h.clock.size, 0);
        assert.equal(h.starts.length, 0);
        h.dispose();
    }
});
