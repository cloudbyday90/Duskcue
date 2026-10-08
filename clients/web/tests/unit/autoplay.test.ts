import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AUTOPLAY_DELAY_MS, createAutoplayController } from '../../src/lib/playback/autoplay.js';

const current = { id: 'episode-one', type: 'episode', series_id: 'series', season_id: 'season', episode_number: 1 };
const next = { ...current, id: 'episode-two', episode_number: 2, availability: { can_play: true } };
const available = { kind: 'available', episode: next, reason: 'next_available' };
const endedContext = { item: current, profileId: 'alex', autoplayEnabled: true, preferencesReady: true };
const clock = { now: () => Date.now(), setTimeout: (callback, delay) => setTimeout(callback, delay), clearTimeout: (timer) => clearTimeout(timer) };

function player(overrides = {}) {
    const changes = [];
    const resolveNext = vi.fn(async () => available);
    const onPlayNext = vi.fn(async (_episode, _options) => {});
    const controller = createAutoplayController({ resolveNext, onPlayNext, onChange: (state) => changes.push(state), clock, ...overrides });
    return { controller, changes, resolveNext, onPlayNext };
}

beforeEach(() => { vi.useFakeTimers(); vi.setSystemTime(0); });
afterEach(() => { vi.useRealTimers(); });

describe('per-instance episode-end timing', () => {
    it('cannot discover or advance without a real ended call', async () => {
        const p = player();
        p.controller.setPreference({ autoplayEnabled: true, preferencesReady: true });
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.controller.getState().phase).toBe('idle');
        expect(p.resolveNext).not.toHaveBeenCalled();
        expect(p.onPlayNext).not.toHaveBeenCalled();
    });

    it('saved Off provides untimed Play next and exactly one manual transition', async () => {
        const p = player();
        await p.controller.ended({ ...endedContext, autoplayEnabled: false });
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', seconds: 0 });
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        await Promise.all([p.controller.playNext(), p.controller.playNext()]);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
        expect(p.onPlayNext.mock.calls[0]).toEqual([next, { signal: expect.any(AbortSignal), mode: 'manual', profileId: 'alex', currentItemId: current.id }]);
    });

    it('unresolved preferences cannot authorize a countdown until confirmed', async () => {
        const p = player();
        await p.controller.ended({ ...endedContext, preferencesReady: false });
        expect(p.controller.getState().phase).toBe('ready');
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        p.controller.setPreference({ autoplayEnabled: true, preferencesReady: true });
        expect(p.controller.getState()).toMatchObject({ phase: 'countdown', seconds: 10 });
    });

    it('advances once at the ten-second deadline and never speaks changing seconds in announcement codes', async () => {
        const p = player();
        await p.controller.ended(endedContext);
        expect(p.controller.getState()).toMatchObject({ phase: 'countdown', remainingMs: AUTOPLAY_DELAY_MS, seconds: 10 });
        await vi.advanceTimersByTimeAsync(9999);
        expect(p.controller.getState().seconds).toBe(1);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        expect(new Set(p.changes.filter((state) => state.phase === 'countdown').map((state) => state.announcement))).toEqual(new Set(['autoplay_countdown']));
        await vi.advanceTimersByTimeAsync(1);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
        expect(p.onPlayNext.mock.calls[0][1].mode).toBe('automatic');
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it.each(['focusWithin', 'disclosureOpen', 'documentHidden', 'nativeBackground'])('pauses %s with exact subsecond remaining time', async (reason) => {
        const p = player();
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(2750);
        p.controller.setPaused({ [reason]: true });
        expect(p.controller.getState()).toMatchObject({ paused: true, remainingMs: 7250, seconds: 8 });
        expect(vi.getTimerCount()).toBe(0);
        await vi.advanceTimersByTimeAsync(20_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        p.controller.setPaused({ [reason]: false });
        await vi.advanceTimersByTimeAsync(7249);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(1);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('does not let clearing one environmental reason override another or a hidden initial document', async () => {
        const p = player();
        p.controller.setPaused({ focusWithin: true, documentHidden: true });
        await p.controller.ended(endedContext);
        expect(vi.getTimerCount()).toBe(0);
        p.controller.setPaused({ focusWithin: false });
        expect(p.controller.getState().paused).toBe(true);
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        p.controller.setPaused({ documentHidden: false });
        await vi.advanceTimersByTimeAsync(10_000);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('Cancel persists across focus, visibility, settings, and repeated ended events with untimed Play next', async () => {
        const p = player();
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(3000);
        p.controller.cancel();
        p.controller.setPaused({ documentHidden: true });
        p.controller.setPaused({ documentHidden: false });
        p.controller.setPreference({ autoplayEnabled: false, preferencesReady: true });
        p.controller.setPreference({ autoplayEnabled: true, preferencesReady: true });
        await p.controller.ended(endedContext);
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', cancelled: true, seconds: 0 });
        expect(p.resolveNext).toHaveBeenCalledTimes(1);
        await vi.advanceTimersByTimeAsync(100_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        expect(vi.getTimerCount()).toBe(0);
        await p.controller.playNext();
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('cancellation during discovery cannot create a timer on later completion', async () => {
        let finish;
        const p = player({ resolveNext: vi.fn(() => new Promise((resolve) => { finish = resolve; })) });
        const pending = p.controller.ended(endedContext);
        p.controller.cancel();
        finish(available);
        await pending;
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', cancelled: true });
        expect(vi.getTimerCount()).toBe(0);
    });

    it.each([{ autoplayEnabled: false, preferencesReady: true }, { autoplayEnabled: true, preferencesReady: false }])('removes running timing when preference authorization changes %j', async (preference) => {
        const p = player();
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(3000);
        p.controller.setPreference(preference);
        expect(p.controller.getState().phase).toBe('ready');
        expect(vi.getTimerCount()).toBe(0);
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
    });

    it('unavailable successor is recoverable but a successful Retry stays untimed', async () => {
        const resolveNext = vi.fn().mockResolvedValueOnce({ kind: 'unavailable', episode: next, reason: 'next_unavailable' }).mockResolvedValue(available);
        const p = player({ resolveNext });
        await p.controller.ended(endedContext);
        expect(p.controller.getState()).toMatchObject({ phase: 'unavailable', nextEpisode: next });
        expect(await p.controller.playNext()).toBe(false);
        await p.controller.retry();
        expect(resolveNext).toHaveBeenCalledTimes(2);
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', cancelled: true });
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        await p.controller.playNext();
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('read failure becomes a recoverable error and retry revalidates rather than auto-playing', async () => {
        const failure = new Error('Fixture access temporarily unavailable');
        const resolveNext = vi.fn().mockRejectedValueOnce(failure).mockResolvedValue(available);
        const p = player({ resolveNext });
        await p.controller.ended(endedContext);
        expect(p.controller.getState()).toMatchObject({ phase: 'error', error: failure });
        await p.controller.retry();
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', cancelled: true });
        expect(vi.getTimerCount()).toBe(0);
    });

    it('failed next playback never repeats automatically and allows an explicit revalidated retry', async () => {
        const failure = new Error('Fixture next file disappeared');
        const onPlayNext = vi.fn().mockRejectedValueOnce(failure).mockResolvedValue(undefined);
        const p = player({ onPlayNext });
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(10_000);
        expect(p.controller.getState()).toMatchObject({ phase: 'error', error: failure, cancelled: true });
        await vi.advanceTimersByTimeAsync(30_000);
        expect(onPlayNext).toHaveBeenCalledTimes(1);
        await p.controller.retry();
        await p.controller.playNext();
        expect(onPlayNext).toHaveBeenCalledTimes(2);
        expect(onPlayNext.mock.calls[1][1].mode).toBe('manual');
    });

    it('new media/profile invalidates a delayed resolver even when the reader ignores abort', async () => {
        let finish;
        let oldSignal;
        const resolveNext = vi.fn().mockImplementationOnce((_item, { signal }) => {
            oldSignal = signal;
            return new Promise((resolve) => { finish = resolve; });
        }).mockResolvedValue(available);
        const p = player({ resolveNext });
        const old = p.controller.ended(endedContext);
        await p.controller.ended({ ...endedContext, profileId: 'morgan' });
        expect(oldSignal.aborted).toBe(true);
        finish({ kind: 'none', episode: null, reason: 'old_scope' });
        await old;
        expect(p.controller.getState()).toMatchObject({ phase: 'countdown', nextEpisode: next, reason: 'next_available' });
    });

    it('dispose aborts delayed work and leaves no timers or late UI callbacks', async () => {
        let finish;
        let signal;
        const p = player({ resolveNext: vi.fn((_item, options) => {
            signal = options.signal;
            return new Promise((resolve) => { finish = resolve; });
        }) });
        const pending = p.controller.ended(endedContext);
        p.controller.dispose();
        const count = p.changes.length;
        finish(available);
        await pending;
        expect(signal.aborted).toBe(true);
        expect(p.changes).toHaveLength(count);
        expect(vi.getTimerCount()).toBe(0);
        expect(p.onPlayNext).not.toHaveBeenCalled();
    });

    it('reset clears timing and permits a new explicit end cycle for the same item', async () => {
        const p = player();
        await p.controller.ended(endedContext);
        p.controller.cancel();
        p.controller.reset();
        await vi.advanceTimersByTimeAsync(30_000);
        expect(p.onPlayNext).not.toHaveBeenCalled();
        expect(p.controller.getState().phase).toBe('idle');
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(10_000);
        expect(p.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('manual/timer races and repeated clicks cannot begin two pending transitions', async () => {
        let finish;
        const onPlayNext = vi.fn((_episode, _options) => new Promise((resolve) => { finish = resolve; }));
        const p = player({ onPlayNext });
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(9999);
        const manual = p.controller.playNext();
        expect(await p.controller.playNext()).toBe(false);
        await vi.advanceTimersByTimeAsync(1000);
        expect(onPlayNext).toHaveBeenCalledTimes(1);
        finish();
        expect(await manual).toBe(true);
        expect(p.controller.getState().phase).toBe('transitioned');
    });

    it('reset aborts a pending transition and rejects its stale completion', async () => {
        let finish;
        const onPlayNext = vi.fn((_episode, _options) => new Promise((resolve) => { finish = resolve; }));
        const p = player({ onPlayNext });
        await p.controller.ended({ ...endedContext, autoplayEnabled: false });
        const pending = p.controller.playNext();
        p.controller.reset();
        expect(onPlayNext.mock.calls[0][1].signal.aborted).toBe(true);
        finish();
        expect(await pending).toBe(false);
        expect(p.controller.getState().phase).toBe('idle');
    });

    it('synchronous pause during a visual tick leaves no stray timer', async () => {
        let controller;
        controller = createAutoplayController({ resolveNext: vi.fn(async () => available), onPlayNext: vi.fn(), clock,
            onChange: (state) => { if (state.phase === 'countdown' && state.seconds === 9 && !state.paused) controller.setPaused({ focusWithin: true }); } });
        await controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(1000);
        expect(controller.getState()).toMatchObject({ paused: true, remainingMs: 9000 });
        expect(vi.getTimerCount()).toBe(0);
    });

    it('separate player instances cannot cancel or advance each other', async () => {
        const alex = player();
        const morgan = player();
        await alex.controller.ended(endedContext);
        await morgan.controller.ended({ ...endedContext, profileId: 'morgan' });
        alex.controller.cancel();
        await vi.advanceTimersByTimeAsync(10_000);
        expect(alex.onPlayNext).not.toHaveBeenCalled();
        expect(morgan.onPlayNext).toHaveBeenCalledTimes(1);
    });

    it('an already queued old timer cannot touch a new profile generation', async () => {
        const callbacks = [];
        let elapsed = 0;
        const queuedClock = { now: () => elapsed, setTimeout: (callback) => { callbacks.push(callback); return callbacks.length; }, clearTimeout: vi.fn() };
        const p = player({ clock: queuedClock });
        await p.controller.ended(endedContext);
        const oldTick = callbacks[0];
        await p.controller.ended({ ...endedContext, profileId: 'morgan' });
        elapsed = 9000;
        oldTick();
        expect(p.controller.getState()).toMatchObject({ phase: 'countdown', seconds: 10, remainingMs: 10_000 });
        expect(callbacks).toHaveLength(2);
        expect(p.onPlayNext).not.toHaveBeenCalled();
    });

    it('a delayed automatic background read cannot cross a changed profile or override Cancel', async () => {
        let finish;
        const beforeAutomaticNext = vi.fn(() => new Promise((resolve) => { finish = resolve; }));
        const p = player({ beforeAutomaticNext });
        await p.controller.ended(endedContext);
        await vi.advanceTimersByTimeAsync(10_000);
        const oldSignal = beforeAutomaticNext.mock.calls[0][0].signal;
        await p.controller.ended({ ...endedContext, profileId: 'morgan' });
        expect(oldSignal.aborted).toBe(true);
        finish(true);
        await Promise.resolve();
        expect(p.onPlayNext).not.toHaveBeenCalled();
        await vi.advanceTimersByTimeAsync(10_000);
        p.controller.cancel();
        finish(true);
        await Promise.resolve();
        expect(p.onPlayNext).not.toHaveBeenCalled();
        expect(p.controller.getState()).toMatchObject({ phase: 'ready', cancelled: true });
    });

    it('requires current profile scope before resolving or timing', async () => {
        const p = player();
        await p.controller.ended({ ...endedContext, profileId: '' });
        expect(p.controller.getState().phase).toBe('error');
        expect(p.resolveNext).not.toHaveBeenCalled();
        expect(vi.getTimerCount()).toBe(0);
    });
});
