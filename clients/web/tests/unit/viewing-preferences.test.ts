import { describe, expect, it, vi } from 'vitest';
import { createViewingPreferencesStore } from '../../src/lib/preferences/store.js';
import { DEFAULT_VIEWING_PREFERENCES, canonicalLanguage, normalizeViewingPreferences } from '../../src/lib/preferences/model.js';
import { readDevicePreferences, writeDevicePreferences } from '../../src/lib/preferences/device.js';
import { hasLegacyAutoplayOff } from '../../src/lib/preferences/storage.js';
import { getDeviceIdentity } from '../../src/lib/device/identity.js';

function deferred<T = any>() {
    let resolve!: (value: T) => void;
    let reject!: (error: Error) => void;
    const promise = new Promise<T>((settle, fail) => { resolve = settle; reject = fail; });
    return { promise, resolve, reject };
}

function memoryStorage(initial: Record<string, string> = {}) {
    const values = new Map(Object.entries(initial));
    return {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => { values.set(key, value); },
        removeItem: (key: string) => { values.delete(key); },
        clear: () => { values.clear(); },
        key: (index: number) => [...values.keys()][index] ?? null,
        get length() { return values.size; },
        values,
    };
}

function response(profileId: string, saved = false, preferences = {}) {
    return {
        profile_id: profileId,
        has_saved_preferences: saved,
        viewing_preferences: { ...DEFAULT_VIEWING_PREFERENCES, ...preferences },
    };
}

const context = { serverOrigin: 'https://media.example/library', userId: 'account-a' };

function makeStore(getPreferences = vi.fn().mockResolvedValue(response('alex')), savePreferences = vi.fn(), storage = memoryStorage()) {
    return createViewingPreferencesStore({ getPreferences, savePreferences, getStorage: () => storage, getDeviceId: () => 'installation-a' });
}

describe('active-profile viewing preferences', () => {
    it('deduplicates simultaneous shell and route loads and honors the existing identity shape', async () => {
        const pending = deferred();
        const transport = vi.fn(() => pending.promise);
        const storage = memoryStorage({ duskcue_device_id: 'installation-a' });
        vi.stubGlobal('localStorage', storage);
        const store = createViewingPreferencesStore({ getPreferences: transport, savePreferences: vi.fn(), getStorage: () => storage, getDeviceId: getDeviceIdentity });
        const first = store.load('alex', context);
        const second = store.load('alex', context);
        expect(first).toBe(second);
        await Promise.resolve();
        expect(transport).toHaveBeenCalledTimes(1);
        pending.resolve(response('alex'));
        const state = await first;
        expect(state.scope).toMatchObject({ serverOrigin: 'https://media.example', userId: 'account-a', deviceId: 'installation-a' });
        await store.load('alex', context);
        expect(transport).toHaveBeenCalledTimes(1);
    });

    it('aborts its old load, clears visible data, and rejects a stale reply even if cancellation is ignored', async () => {
        const old = deferred();
        const next = deferred();
        const transport = vi.fn().mockReturnValueOnce(old.promise).mockReturnValueOnce(next.promise);
        const store = makeStore(transport);
        const first = store.load('alex', context);
        const rejection = expect(first).rejects.toMatchObject({ name: 'AbortError' });
        await Promise.resolve();
        const oldSignal = transport.mock.calls[0][0].signal;
        const second = store.load('morgan', context);
        expect(oldSignal.aborted).toBe(true);
        expect(store.getSnapshot()).toMatchObject({ profileId: 'morgan', status: 'loading', preferences: null });
        old.resolve(response('alex', true, { audio_language: 'fr' }));
        await rejection;
        expect(store.getSnapshot()).toMatchObject({ profileId: 'morgan', preferences: null });
        next.resolve(response('morgan'));
        await second;
        expect(store.getSnapshot().preferences.audio_language).toBeNull();
    });

    it('rejects a response for the wrong active profile and retries after failure', async () => {
        const transport = vi.fn().mockResolvedValueOnce(response('morgan')).mockResolvedValueOnce(response('alex'));
        const store = makeStore(transport);
        await expect(store.load('alex', context)).rejects.toMatchObject({ name: 'AbortError' });
        expect(store.getSnapshot()).toMatchObject({ status: 'error', preferences: null, devicePreferences: null });
        await store.load('alex', context);
        expect(store.getSnapshot().status).toBe('ready');
    });

    it('never exposes the previous profile when the new load fails', async () => {
        const transport = vi.fn().mockResolvedValueOnce(response('alex', true, { prefer_sdh: true })).mockRejectedValueOnce(new Error('Offline'));
        const store = makeStore(transport);
        await store.load('alex', context);
        await expect(store.load('morgan', context)).rejects.toThrow('Offline');
        expect(store.getSnapshot()).toMatchObject({ profileId: 'morgan', status: 'error', preferences: null, devicePreferences: null });
    });

    it('keeps saved defaults distinct and uses only valid legacy Off for an unsaved profile', async () => {
        const storage = memoryStorage({ duskcue_prefs: JSON.stringify({ autoplay: false, audioLanguage: 'fr', subtitleLanguage: 'de' }) });
        const transport = vi.fn().mockResolvedValueOnce(response('alex')).mockResolvedValueOnce(response('morgan', true));
        const store = makeStore(transport, vi.fn(), storage);
        await store.load('alex', context);
        expect(store.getSnapshot()).toMatchObject({ hasSavedPreferences: false, legacyAutoplayOff: true, preferences: { autoplay_next_episode: false, audio_language: null, subtitle_language: null } });
        await store.load('morgan', context);
        expect(store.getSnapshot()).toMatchObject({ hasSavedPreferences: true, legacyAutoplayOff: false, preferences: { autoplay_next_episode: true } });
        expect(JSON.parse(storage.getItem('duskcue_prefs')!)).toMatchObject({ autoplay: false, audioLanguage: 'fr' });
    });

    it('keeps a failed save draft separate from the confirmed baseline and permits retry after a synchronous transport failure', async () => {
        const saves = vi.fn().mockImplementationOnce(() => { throw new Error('Save failed'); }).mockResolvedValueOnce(response('alex', true, { audio_language: 'fr' }));
        const store = makeStore(undefined, saves);
        await store.load('alex', context);
        await expect(store.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fra' })).rejects.toThrow('Save failed');
        expect(store.getSnapshot()).toMatchObject({ status: 'ready', hasSavedPreferences: false, preferences: { audio_language: null } });
        await store.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'fra' });
        expect(saves.mock.calls[1][1].audio_language).toBe('fr');
        expect(store.getSnapshot()).toMatchObject({ hasSavedPreferences: true, preferences: { audio_language: 'fr' } });
    });

    it('rejects a late save after a profile switch without altering the new profile', async () => {
        const pending = deferred();
        const saves = vi.fn((_profileId: string, _preferences: unknown, _options: { signal: AbortSignal }) => pending.promise);
        const loads = vi.fn().mockResolvedValueOnce(response('alex')).mockResolvedValueOnce(response('morgan'));
        const store = makeStore(loads, saves);
        await store.load('alex', context);
        const saving = store.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, audio_language: 'de' });
        const rejection = expect(saving).rejects.toMatchObject({ name: 'AbortError' });
        await Promise.resolve();
        const signal = saves.mock.calls[0][2].signal;
        await store.load('morgan', context);
        expect(signal.aborted).toBe(true);
        pending.resolve(response('alex', true, { audio_language: 'de' }));
        await rejection;
        expect(store.getSnapshot()).toMatchObject({ profileId: 'morgan', status: 'ready', preferences: { audio_language: null } });
        await expect(store.save('alex', DEFAULT_VIEWING_PREFERENCES)).rejects.toMatchObject({ name: 'AbortError' });
    });

    it('does not abort an active same-scope save to satisfy a reload', async () => {
        const pending = deferred();
        const saves = vi.fn((_profileId: string, _preferences: unknown, _options: { signal: AbortSignal }) => pending.promise);
        const store = makeStore(undefined, saves);
        await store.load('alex', context);
        const saving = store.save('alex', { ...DEFAULT_VIEWING_PREFERENCES, autoplay_next_episode: false });
        await Promise.resolve();
        const loading = store.load('alex', context, { force: true });
        expect(loading).toBe(saving);
        expect(saves.mock.calls[0][2].signal.aborted).toBe(false);
        pending.resolve(response('alex', true, { autoplay_next_episode: false }));
        await loading;
        expect(store.getSnapshot().preferences.autoplay_next_episode).toBe(false);
    });

    it('invalidates on server or account changes even when the profile id matches', async () => {
        const transport = vi.fn().mockResolvedValue(response('alex'));
        const store = makeStore(transport);
        await store.load('alex', context);
        await store.load('alex', { ...context, userId: 'account-b' });
        await store.load('alex', { ...context, serverOrigin: 'https://another.example' });
        expect(transport).toHaveBeenCalledTimes(3);
        store.invalidate();
        expect(store.getSnapshot()).toMatchObject({ status: 'idle', profileId: null, preferences: null });
    });

    it('reports device storage failure while retaining a confirmed profile save', async () => {
        const storage = memoryStorage();
        storage.setItem = () => { throw new Error('Storage unavailable'); };
        const store = makeStore(undefined, vi.fn().mockResolvedValue(response('alex', true)), storage);
        await store.load('alex', context);
        await store.save('alex', DEFAULT_VIEWING_PREFERENCES);
        expect(() => store.saveDevice('alex', { quality_mode: 'manual', max_streaming_bitrate: 6000000 })).toThrow('Storage unavailable');
        expect(store.getSnapshot()).toMatchObject({ hasSavedPreferences: true, devicePreferences: { quality_mode: 'auto' } });
    });
});

describe('preference validation and local device scope', () => {
    it.each(['false', null, [], { autoplay: 'false' }, { autoplay: 0 }, { autoplay: true }])('ignores invalid or enabled legacy autoplay %j', (legacy) => {
        expect(hasLegacyAutoplayOff(memoryStorage({ duskcue_prefs: JSON.stringify(legacy) }))).toBe(false);
    });

    it('falls back safely for corrupt legacy and device storage', () => {
        const storage = memoryStorage({ duskcue_prefs: '{' });
        expect(hasLegacyAutoplayOff(storage)).toBe(false);
        const scope = { serverOrigin: 'https://media.example', userId: 'a', deviceId: 'd' };
        writeDevicePreferences(scope, { quality_mode: 'auto', max_streaming_bitrate: null }, storage);
        const key = [...storage.values.keys()].find((value) => value.startsWith('duskcue_viewing_device_prefs_v1:'))!;
        storage.setItem(key, '{');
        expect(readDevicePreferences(scope, storage)).toEqual({ quality_mode: 'auto', max_streaming_bitrate: null });
    });

    it('shares quality across profiles on the same device but isolates server, account, and installation', () => {
        const storage = memoryStorage();
        const scope = { serverOrigin: 'https://media.example', userId: 'a', deviceId: 'd', profileId: 'alex' };
        const quality = { quality_mode: 'manual', max_streaming_bitrate: 6000000 };
        writeDevicePreferences(scope, quality, storage);
        expect(readDevicePreferences({ ...scope, profileId: 'morgan' }, storage)).toEqual(quality);
        for (const other of [{ ...scope, userId: 'b' }, { ...scope, serverOrigin: 'https://other.example' }, { ...scope, deviceId: 'e' }]) {
            expect(readDevicePreferences(other, storage).quality_mode).toBe('auto');
        }
        expect(() => writeDevicePreferences({ ...scope, deviceId: null }, quality, storage)).toThrow('could not be stored');
    });

    it('rejects invalid device bitrate limits instead of persisting them', () => {
        for (const bitrate of [0, -1, 1.5, '6000000', 1000000001, null]) {
            expect(() => writeDevicePreferences({ serverOrigin: 'https://media.example', userId: 'a', deviceId: 'd' }, { quality_mode: 'manual', max_streaming_bitrate: bitrate }, memoryStorage())).toThrow();
        }
    });

    it('normalizes scanner language aliases, rejects display labels, and requires a subtitle language', () => {
        expect(canonicalLanguage(' FRE ')).toBe('fr');
        expect(canonicalLanguage('eng')).toBe('en');
        expect(canonicalLanguage('ast')).toBe('ast');
        for (const language of ['English', 'en-US', 'und', 'mul', 'zxx', '', 'x'.repeat(33)]) {
            expect(() => canonicalLanguage(language)).toThrow();
        }
        expect(() => normalizeViewingPreferences({ ...DEFAULT_VIEWING_PREFERENCES, subtitle_mode: 'always' })).toThrow('required');
        expect(() => normalizeViewingPreferences({ ...DEFAULT_VIEWING_PREFERENCES, autoplay_next_episode: 'false' })).toThrow();
    });
});
