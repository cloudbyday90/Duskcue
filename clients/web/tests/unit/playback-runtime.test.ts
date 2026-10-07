import { describe, expect, it, vi } from 'vitest';
import { get, writable } from 'svelte/store';
import { createPlaybackRuntime } from '../../src/lib/playback/runtime.js';
import { createViewingPreferencesStore } from '../../src/lib/preferences/store.js';
import { DEFAULT_VIEWING_PREFERENCES, normalizePreferenceScope } from '../../src/lib/preferences/model.js';
import { writeDevicePreferences } from '../../src/lib/preferences/device.js';
import { createTrackOverride } from '../../src/lib/playback/tracks.js';
import { playbackTitleDestination } from '../../src/lib/playback/entry.js';

const context = { serverOrigin: 'https://media.example/library', userId: 'account-a' };
const item = { id: 'episode-1', type: 'episode', series_id: 'series-1', season_id: 'season-1', runtime_seconds: 1800 };
const nextItem = { ...item, id: 'episode-2' };
const file = {
    id: 'file-1', is_healthy: true,
    additional_streams: {
        audio: [
            { index: 2, language: 'eng', codec: 'aac', disposition: { default: 1, visual_impaired: 0 } },
            { index: 5, language: 'fra', codec: 'aac', disposition: { default: 0, visual_impaired: 1 } },
        ],
        subtitles: [
            { index: 8, language: 'eng', codec: 'subrip', disposition: { hearing_impaired: 1 } },
            { index: 11, language: 'fra', codec: 'subrip', disposition: { hearing_impaired: 0 } },
            { index: 13, language: 'fra', codec: 'hdmv_pgs_subtitle' },
        ],
    },
};
const nextFile = {
    id: 'file-2', is_healthy: true,
    additional_streams: {
        audio: [
            { index: 17, language: 'eng', codec: 'aac', disposition: { default: 1, visual_impaired: 0 } },
            { index: 23, language: 'fra', codec: 'aac', disposition: { default: 0, visual_impaired: 1 } },
        ],
        subtitles: [
            { index: 19, language: 'eng', codec: 'subrip', disposition: { hearing_impaired: 1 } },
            { index: 25, language: 'fra', codec: 'subrip', disposition: { hearing_impaired: 0 } },
        ],
    },
};
const entry = { item, file, startPositionMs: 120000, destination: '/media/series-1?season=season-1&episode=episode-1&from=%2Fsearch%3Fq%3Dnight' };
const nextEntry = { item: nextItem, file: nextFile, startPositionMs: 300000, destination: '/media/series-1?season=season-1&episode=episode-2' };

function deferred<T = any>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((settle, fail) => { resolve = settle; reject = fail; });
    return { promise, resolve, reject };
}

function memoryStorage(initial: Record<string, string> = {}) {
    const values = new Map(Object.entries(initial));
    return { getItem: (key: string) => values.get(key) ?? null, setItem: (key: string, value: string) => { values.set(key, value); } };
}

function response(value = {}, saved = true, profileId = 'alex') {
    return { profile_id: profileId, has_saved_preferences: saved, viewing_preferences: { ...DEFAULT_VIEWING_PREFERENCES, ...value } };
}

function fakePlayer(decision = 'direct_play') {
    const store = writable<any>({ sessionId: null, loading: false, positionMs: 0, volume: 0.6, playbackRate: 1.5, isFullscreen: true, isPlaying: false });
    let generation = 0;
    let session = 0;
    const startResult = vi.fn(async (_item: any, _fileId: string, _options: any): Promise<any> => ({ session_id: `session-${++session}`, stream_decision: decision }));
    const seekResult = vi.fn(async (_position: number, _options: any): Promise<any> => ({ stream_url: 'replacement.m3u8' }));
    const play = vi.fn(async (media: any, fileId: string, options: any) => {
        const token = ++generation;
        const abort = () => { if (generation === token) { generation++; store.update((state) => ({ ...state, loading: false })); } };
        options.signal?.addEventListener('abort', abort, { once: true });
        store.update((state) => ({ ...state, loading: true }));
        try {
            const result = await startResult(media, fileId, options);
            if (generation !== token || options.signal?.aborted) return null;
            store.update((state) => result ? { ...state, sessionId: result.session_id, streamDecision: result.stream_decision, mediaItem: media, mediaFileId: fileId, loading: false, isPlaying: true, positionMs: options.startPositionMs } : { ...state, loading: false });
            return result;
        } finally {
            options.signal?.removeEventListener('abort', abort);
        }
    });
    const seek = vi.fn(async (value: number, options: any) => {
        const token = ++generation;
        const result = await seekResult(value, options);
        if (token !== generation || options.signal?.aborted) return null;
        store.update((state) => ({ ...state, positionMs: value, streamOffsetMs: value }));
        return result;
    });
    const setPlaying = vi.fn((value: boolean) => { store.update((state) => ({ ...state, isPlaying: value })); });
    const stop = vi.fn();
    return {
        subscribe: store.subscribe, play, seek, setPlaying, stop, startResult, seekResult,
        update: (value: any) => store.update((state) => ({ ...state, ...value })),
        replaceSession: (id: string) => { generation++; store.update((state) => ({ ...state, sessionId: id, loading: false })); },
    };
}

function harness({ value = {}, saved = true, storage = memoryStorage(), getPreferences = undefined, savePreferences = undefined, loadEntry = undefined, authenticated = false, decision = 'direct_play' }: any = {}) {
    const reads = getPreferences || vi.fn(async (_options: any) => response(value, saved));
    const saves = savePreferences || vi.fn(async (_id: string, preferences: any, _options: any) => response(preferences));
    const preferences = createViewingPreferencesStore({ getPreferences: reads, savePreferences: saves, getStorage: () => storage, getDeviceId: () => 'installation-a' });
    const player = fakePlayer(decision);
    const entries = new Map([[item.id, entry], [nextItem.id, nextEntry]]);
    const loader = loadEntry || vi.fn(async (id: string, options: any) => {
        const actual = entries.get(id);
        if (!actual) throw new Error('No access');
        if (options.fileId && options.fileId !== actual.file.id) throw new Error('File unavailable');
        return { ...actual, destination: playbackTitleDestination(actual.item, options.returnTo) };
    });
    const onChange = vi.fn((_state: any) => {});
    const runtime = createPlaybackRuntime({ player, preferences, loadEntry: loader, requiresAuthenticatedSource: () => authenticated, onChange });
    return { runtime, player, preferences, reads, saves, loader, entries, onChange, storage };
}

async function started(options: any = {}) {
    const result = harness(options);
    await result.runtime.start(entry, { profileId: 'alex', context });
    return result;
}

describe('per-player playback runtime', () => {
    it('waits for confirmed preferences before starting and reads the full file tracks', async () => {
        const pending = deferred();
        const reads = vi.fn((_options: any) => pending.promise);
        const h = harness({ getPreferences: reads });
        const running = h.runtime.start(entry, { profileId: 'alex', context });
        await Promise.resolve();
        expect(h.player.play).not.toHaveBeenCalled();
        expect(h.runtime.getState()).toMatchObject({ loading: true, preferencesReady: false, autoplayEnabled: false });
        pending.resolve(response());
        await running;
        expect(h.runtime.getState()).toMatchObject({ item, file, preferencesReady: true, autoplayEnabled: true, loading: false });
        expect(h.runtime.getState().tracks.audio.map((track) => track.index)).toEqual([2, 5]);
        expect(h.runtime.getState().tracks.subtitles.find((track) => track.index === 13)?.selectable).toBe(false);
        expect(h.player.play.mock.calls[0][2]).toMatchObject({ startPositionMs: 120000, audioStreamIndex: null, subtitleStreamIndex: null, forceTranscode: false });
        h.runtime.dispose();
    });

    it('loads preferences once across restart and a fresh next episode transition', async () => {
        const h = await started();
        h.player.update({ positionMs: 410000 });
        await h.runtime.restart();
        await h.runtime.transition(nextItem, { profileId: 'alex' });
        expect(h.reads).toHaveBeenCalledTimes(1);
        expect(h.loader).toHaveBeenCalledWith(nextItem.id, expect.objectContaining({ signal: expect.any(AbortSignal), returnTo: entry.destination }));
        expect(h.player.play.mock.calls[1][2].startPositionMs).toBe(410000);
        expect(h.player.play.mock.calls[2][2].startPositionMs).toBe(300000);
        const destination = new URL(h.runtime.getState().destination, 'https://media.example');
        expect(destination.searchParams.get('episode')).toBe(nextItem.id);
        expect(destination.searchParams.get('from')).toBe('/search?q=night');
        h.runtime.dispose();
    });

    it.each([{ saved: true, value: { autoplay_next_episode: false } }, { saved: false, storage: memoryStorage({ duskcue_prefs: JSON.stringify({ autoplay: false }) }) }])('honors saved or valid legacy autoplay Off before playback (%j)', async (options) => {
        const h = await started(options);
        expect(h.runtime.getState()).toMatchObject({ preferencesReady: true, autoplayEnabled: false });
        h.runtime.dispose();
    });

    it('uses saved device quality without writing a one-off choice', async () => {
        const storage = memoryStorage();
        const deviceScope = normalizePreferenceScope('alex', context, 'installation-a');
        writeDevicePreferences(deviceScope, { quality_mode: 'manual', max_streaming_bitrate: 6000000 }, storage);
        const h = await started({ storage });
        expect(h.player.play.mock.calls[0][2]).toMatchObject({ qualityMode: 'manual', maxBitrate: 6000000 });
        await h.runtime.restart({ quality: { quality_mode: 'maximum', max_streaming_bitrate: null } });
        expect(h.runtime.getState().quality.quality_mode).toBe('maximum');
        expect(h.preferences.getSnapshot().devicePreferences).toEqual({ quality_mode: 'manual', max_streaming_bitrate: 6000000 });
        expect(h.saves).not.toHaveBeenCalled();
        await h.runtime.transition(nextItem);
        expect(h.player.play.mock.calls[2][2]).toMatchObject({ qualityMode: 'maximum', maxBitrate: null });
        await h.runtime.restart({ quality: null });
        expect(h.runtime.getState().quality.quality_mode).toBe('manual');
        h.runtime.dispose();
    });

    it('blocks playback on a preference error and retries against fresh entry data', async () => {
        const reads = vi.fn().mockRejectedValueOnce(new Error('Preferences offline')).mockResolvedValueOnce(response());
        const h = harness({ getPreferences: reads });
        await expect(h.runtime.start(entry, { profileId: 'alex', context })).rejects.toThrow('Preferences offline');
        expect(h.runtime.getState()).toMatchObject({ loading: false, preferencesReady: false, autoplayEnabled: false });
        expect(h.player.play).not.toHaveBeenCalled();
        await h.runtime.retry();
        expect(h.reads).toHaveBeenCalledTimes(2);
        expect(h.loader).toHaveBeenCalledWith(item.id, expect.objectContaining({ fileId: file.id, returnTo: entry.destination }));
        expect(h.runtime.getState().error).toBeNull();
        h.runtime.dispose();
    });

    it('rejects unconfirmed or mismatched initial profile contexts', async () => {
        const h = harness({ getPreferences: vi.fn(async (_options: any) => response({}, true, 'morgan')) });
        await expect(h.runtime.start(entry, { profileId: 'alex', context })).rejects.toMatchObject({ name: 'AbortError' });
        expect(h.player.play).not.toHaveBeenCalled();
        expect(h.runtime.getState().autoplayEnabled).toBe(false);
        await expect(h.runtime.start(entry, { profileId: 'alex', context: { serverOrigin: 'https://media.example' } })).rejects.toThrow('confirmed profile');
        expect(h.runtime.getState().preferencesReady).toBe(false);
        h.runtime.dispose();
    });

    it.each(['transcode', 'direct_stream'])('waits for real resume seek before publishing %s playback', async (decision) => {
        const h = harness({ decision });
        const pending = deferred();
        h.player.seekResult.mockReturnValueOnce(pending.promise);
        const running = h.runtime.start(entry, { profileId: 'alex', context });
        await vi.waitFor(() => expect(h.player.seek).toHaveBeenCalled());
        expect(h.runtime.getState()).toMatchObject({ loading: true, item: null, destination: '' });
        expect(h.player.seek).toHaveBeenCalledWith(120000, { signal: h.player.play.mock.calls[0][2].signal });
        pending.resolve({ stream_url: 'actual-resume.m3u8' });
        await running;
        expect(h.runtime.getState()).toMatchObject({ loading: false, item, destination: entry.destination });
        expect(get(h.player).streamOffsetMs).toBe(120000);
        h.runtime.dispose();
    });

    it.each(['direct_play', 'transcode'])('retains direct DOM resume and avoids an unnecessary zero-position seek (%s)', async (decision) => {
        const h = harness({ decision });
        await h.runtime.start({ ...entry, startPositionMs: decision === 'transcode' ? 0 : entry.startPositionMs }, { profileId: 'alex', context });
        expect(h.player.seek).not.toHaveBeenCalled();
        h.runtime.dispose();
    });

    it('forces the existing transcode path for authenticated media sources', async () => {
        const h = await started({ authenticated: true });
        expect(h.player.play.mock.calls[0][2]).toMatchObject({ forceTranscode: true, audioStreamIndex: null });
        await h.runtime.restart();
        await h.runtime.transition(nextItem);
        expect(h.player.play.mock.calls.every((call) => call[2].forceTranscode)).toBe(true);
        h.runtime.dispose();
    });

    it.each([new Error('Seek offline'), undefined, null])('keeps failed resume recoverable and pauses only its current session (%s)', async (failure) => {
        const h = harness({ decision: 'transcode' });
        if (failure instanceof Error) h.player.seekResult.mockRejectedValueOnce(failure);
        else h.player.seekResult.mockResolvedValueOnce(failure);
        await expect(h.runtime.start(entry, { profileId: 'alex', context })).rejects.toBeTruthy();
        expect(h.runtime.getState()).toMatchObject({ loading: false, item: null, destination: '' });
        expect(h.player.setPlaying).toHaveBeenCalledWith(false);
        expect(get(h.player).isPlaying).toBe(false);
        await h.runtime.retry();
        expect(h.runtime.getState()).toMatchObject({ error: null, item, loading: false });
        h.runtime.dispose();
    });

    it('uses current position after file revalidation and preserves changed speed, volume, and fullscreen', async () => {
        const h = await started({ decision: 'transcode' });
        const pending = deferred();
        h.loader.mockReturnValueOnce(pending.promise);
        const running = h.runtime.restart({ quality: { quality_mode: 'maximum', max_streaming_bitrate: null } });
        h.player.update({ positionMs: 650123, playbackRate: 2, volume: 0.35, isFullscreen: false });
        pending.resolve({ ...entry, startPositionMs: 10 });
        await running;
        expect(h.player.play.mock.calls[1][2].startPositionMs).toBe(650123);
        expect(h.player.seek.mock.calls[1][0]).toBe(650123);
        expect(get(h.player)).toMatchObject({ playbackRate: 2, volume: 0.35, isFullscreen: false, positionMs: 650123 });
        h.runtime.dispose();
    });

    it('matches saved actual tracks and reports inaccessible preferred-track fallbacks', async () => {
        const h = await started({ value: { audio_language: 'fr', prefer_audio_description: true, subtitle_mode: 'always', subtitle_language: 'en', prefer_sdh: true } });
        expect(h.player.play.mock.calls[0][2]).toMatchObject({ audioStreamIndex: 5, subtitleStreamIndex: 8, forceTranscode: true });
        expect(h.runtime.getState().fallback).toEqual({ audio: false, subtitle: false });
        await h.preferences.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'de' });
        await h.runtime.restart();
        expect(h.runtime.getState().fallback.audio).toBe(true);
        expect(h.player.play.mock.calls[1][2]).toMatchObject({ audioStreamIndex: null, subtitleStreamIndex: null, forceTranscode: false });
        h.runtime.dispose();
    });

    it('carries semantic one-off tracks across changed indexes and supports explicit Off/default and clearing', async () => {
        const h = await started({ value: { subtitle_mode: 'always', subtitle_language: 'en' } });
        const tracks = h.runtime.getState().tracks;
        await h.runtime.restart({
            audio: createTrackOverride('audio', tracks.audio.find((track) => track.index === 5), file.id),
            subtitle: createTrackOverride('subtitle', tracks.subtitles.find((track) => track.index === 11), file.id),
        });
        await h.runtime.transition(nextItem);
        expect(h.player.play.mock.calls[2][2]).toMatchObject({ audioStreamIndex: 23, subtitleStreamIndex: 25 });
        expect(h.runtime.getState().overrides.audio.fileId).toBe(file.id);
        await h.runtime.restart({ audio: createTrackOverride('audio', null, nextFile.id), subtitle: createTrackOverride('subtitle', null, nextFile.id) });
        expect(h.player.play.mock.calls[3][2]).toMatchObject({ audioStreamIndex: null, subtitleStreamIndex: null });
        await h.runtime.restart({ audio: null, subtitle: null });
        expect(h.player.play.mock.calls[4][2]).toMatchObject({ audioStreamIndex: null, subtitleStreamIndex: 19 });
        expect(h.saves).not.toHaveBeenCalled();
        h.runtime.dispose();
    });

    it('marks unknown-language overrides as fallback when the file changes', async () => {
        const h = harness();
        const unknownFile = { ...file, additional_streams: { ...file.additional_streams, audio: [{ index: 2, codec: 'aac' }] } };
        h.entries.set(item.id, { ...entry, file: unknownFile } as any);
        await h.runtime.start({ ...entry, file: unknownFile }, { profileId: 'alex', context });
        await h.runtime.restart({ audio: createTrackOverride('audio', h.runtime.getState().tracks.audio[0], file.id) });
        await h.runtime.transition(nextItem);
        expect(h.runtime.getState().fallback.audio).toBe(true);
        expect(h.player.play.mock.calls[2][2].audioStreamIndex).toBeNull();
        h.runtime.dispose();
    });

    it('applies confirmed autoplay saves without refetching or changing the running tracks', async () => {
        const pending = deferred();
        const saves = vi.fn((_id: string, _preferences: any, _options: any) => pending.promise);
        const h = await started({ savePreferences: saves });
        const oldSelection = h.runtime.getState().selection;
        const saving = h.preferences.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, autoplay_next_episode: false, audio_language: 'fr' });
        expect(h.runtime.getState().autoplayEnabled).toBe(true);
        pending.resolve(response({ autoplay_next_episode: false, audio_language: 'fr' }));
        await saving;
        expect(h.runtime.getState().autoplayEnabled).toBe(false);
        expect(h.runtime.getState().selection).toBe(oldSelection);
        expect(h.reads).toHaveBeenCalledTimes(1);
        expect(h.player.play).toHaveBeenCalledTimes(1);
        h.runtime.dispose();
    });

    it('disables autoplay and cancels pending work when the confirmed scope is invalidated', async () => {
        const h = await started();
        const pending = deferred();
        h.loader.mockReturnValueOnce(pending.promise);
        const running = h.runtime.transition(nextItem);
        const rejection = expect(running).rejects.toMatchObject({ name: 'AbortError' });
        const signal = h.loader.mock.calls[0][1].signal;
        h.preferences.invalidate();
        expect(signal.aborted).toBe(true);
        expect(h.runtime.getState()).toMatchObject({ preferencesReady: false, autoplayEnabled: false, loading: false });
        pending.resolve(nextEntry);
        await rejection;
        expect(h.player.play).toHaveBeenCalledTimes(1);
        h.runtime.dispose();
    });

    it('rejects a cross-profile transition without transport or entry reads', async () => {
        const h = await started();
        await expect(h.runtime.transition(nextItem, { profileId: 'morgan' })).rejects.toMatchObject({ name: 'AbortError' });
        expect(h.loader).not.toHaveBeenCalled();
        expect(h.player.play).toHaveBeenCalledTimes(1);
        h.runtime.dispose();
    });

    it.each(['other-series', 'other-season', 'movie'])('rejects an unrelated next identity %s', async (identity) => {
        const h = await started();
        h.entries.set(nextItem.id, { ...nextEntry, item: { ...nextItem, series_id: identity === 'other-series' ? identity : item.series_id, season_id: identity === 'other-season' ? identity : item.season_id, type: identity === 'movie' ? 'movie' : 'episode' } });
        await expect(h.runtime.transition(nextItem)).rejects.toMatchObject({ code: 'TRANSITION_UNAVAILABLE' });
        expect(h.player.play).toHaveBeenCalledTimes(1);
        expect(h.runtime.getState().item.id).toBe(item.id);
        h.runtime.dispose();
    });

    it('starts completed next episodes at zero and rejects unavailable actual files', async () => {
        const h = await started();
        h.entries.set(nextItem.id, { ...nextEntry, startPositionMs: 0 });
        await h.runtime.transition(nextItem);
        expect(h.player.play.mock.calls[1][2].startPositionMs).toBe(0);
        h.entries.set(nextItem.id, { ...nextEntry, file: { ...nextFile, is_healthy: false } });
        await expect(h.runtime.restart()).rejects.toMatchObject({ code: 'FILES_UNAVAILABLE' });
        expect(h.player.play).toHaveBeenCalledTimes(2);
        h.runtime.dispose();
    });

    it('retains a failed next choice for fresh explicit retry without falsely publishing it', async () => {
        const h = await started();
        h.loader.mockRejectedValueOnce(new Error('No healthy file'));
        await expect(h.runtime.transition(nextItem)).rejects.toThrow('No healthy file');
        expect(h.runtime.getState()).toMatchObject({ item, destination: entry.destination, loading: false });
        await h.runtime.retry();
        expect(h.loader).toHaveBeenCalledTimes(2);
        expect(h.runtime.getState()).toMatchObject({ item: nextItem, error: null });
        h.runtime.dispose();
    });

    it.each([null, new Error('Start unavailable')])('rejects failed playback starts instead of treating them as successful transitions (%s)', async (failure) => {
        const h = await started();
        if (failure instanceof Error) h.player.startResult.mockRejectedValueOnce(failure);
        else h.player.startResult.mockResolvedValueOnce(failure);
        await expect(h.runtime.transition(nextItem)).rejects.toBeTruthy();
        expect(h.runtime.getState()).toMatchObject({ item, destination: entry.destination, loading: false });
        h.runtime.dispose();
    });

    it('ignores old entry reads when a newer operation has completed', async () => {
        const h = await started();
        const pending = deferred();
        h.loader.mockReturnValueOnce(pending.promise);
        const old = h.runtime.restart();
        const rejection = expect(old).rejects.toMatchObject({ name: 'AbortError' });
        await h.runtime.transition(nextItem);
        pending.resolve(entry);
        await rejection;
        expect(h.player.play).toHaveBeenCalledTimes(2);
        expect(h.runtime.getState()).toMatchObject({ item: nextItem, error: null });
        h.runtime.dispose();
    });

    it('disposes while preferences load without invalidating the shared facade or starting playback', async () => {
        const pending = deferred();
        const h = harness({ getPreferences: vi.fn((_options: any) => pending.promise) });
        const running = h.runtime.start(entry, { profileId: 'alex', context });
        const rejection = expect(running).rejects.toMatchObject({ name: 'AbortError' });
        await Promise.resolve();
        h.runtime.dispose();
        const changes = h.onChange.mock.calls.length;
        pending.resolve(response());
        await rejection;
        expect(h.player.play).not.toHaveBeenCalled();
        expect(h.preferences.getSnapshot().status).toBe('ready');
        expect(h.onChange.mock.calls.length).toBe(changes);
        expect(h.player.stop).not.toHaveBeenCalled();
    });

    it('passes external cancellation to transport and ignores late seek failures without pausing a newer session', async () => {
        const h = harness({ decision: 'transcode' });
        const pending = deferred();
        h.player.seekResult.mockReturnValueOnce(pending.promise);
        const controller = new AbortController();
        const running = h.runtime.start(entry, { profileId: 'alex', context, signal: controller.signal });
        const rejection = expect(running).rejects.toMatchObject({ name: 'AbortError' });
        await vi.waitFor(() => expect(h.player.seek).toHaveBeenCalled());
        const operationSignal = h.player.play.mock.calls[0][2].signal;
        controller.abort();
        h.player.replaceSession('newer-session');
        pending.resolve(undefined);
        await rejection;
        expect(operationSignal.aborted).toBe(true);
        expect(h.player.setPlaying).not.toHaveBeenCalled();
        expect(h.player.stop).not.toHaveBeenCalled();
        expect(get(h.player).sessionId).toBe('newer-session');
        h.runtime.dispose();
    });

    it('does not restart a player session that was replaced while entry reads were pending', async () => {
        const h = await started();
        const pending = deferred();
        h.loader.mockReturnValueOnce(pending.promise);
        const running = h.runtime.restart();
        const rejection = expect(running).rejects.toMatchObject({ name: 'AbortError' });
        h.player.replaceSession('newer-session');
        pending.resolve(entry);
        await rejection;
        expect(h.player.play).toHaveBeenCalledTimes(1);
        expect(get(h.player).sessionId).toBe('newer-session');
        h.runtime.dispose();
    });

    it('cleans up one instance without stopping or corrupting a newer instance', async () => {
        const h = await started();
        const pending = deferred();
        h.player.startResult.mockReturnValueOnce(pending.promise);
        const old = h.runtime.restart();
        const rejection = expect(old).rejects.toMatchObject({ name: 'AbortError' });
        await vi.waitFor(() => expect(h.player.play).toHaveBeenCalledTimes(2));
        const newer = createPlaybackRuntime({ player: h.player, preferences: h.preferences, loadEntry: h.loader, requiresAuthenticatedSource: () => false });
        await newer.start(nextEntry, { profileId: 'alex', context });
        const newSessionId = get(h.player).sessionId;
        h.runtime.dispose();
        pending.resolve({ session_id: 'late-old-session', stream_decision: 'direct_play' });
        await rejection;
        expect(get(h.player).sessionId).toBe(newSessionId);
        expect(newer.getState()).toMatchObject({ item: nextItem, error: null });
        expect(h.player.stop).not.toHaveBeenCalled();
        expect(h.player.setPlaying).not.toHaveBeenCalled();
        newer.dispose();
    });
});
