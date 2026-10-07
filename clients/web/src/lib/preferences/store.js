/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { writable } from 'svelte/store';
import { normalizePreferenceScope, normalizeViewingPreferences, scopeChangedError } from './model.js';
import { browserStorage, hasLegacyAutoplayOff } from './storage.js';
import { readDevicePreferences, writeDevicePreferences } from './device.js';

function emptyState() {
    return {
        profileId: null,
        scope: null,
        status: 'idle',
        hasSavedPreferences: false,
        preferences: null,
        devicePreferences: null,
        legacyAutoplayOff: false,
        error: null,
    };
}

export function createViewingPreferencesStore({ getPreferences, savePreferences, getStorage = browserStorage, getDeviceId = () => null }) {
    let snapshot = emptyState();
    const store = writable(snapshot);
    let generation = 0;
    let loadFlight = null;
    let saveFlight = null;

    function publish(next) {
        snapshot = next;
        store.set(snapshot);
    }

    function invalidate() {
        generation += 1;
        loadFlight?.controller.abort();
        saveFlight?.controller.abort();
        loadFlight = null;
        saveFlight = null;
        publish(emptyState());
    }

    function requireCurrent(capturedGeneration, key, response = null) {
        if (capturedGeneration !== generation || snapshot.scope?.key !== key) throw scopeChangedError();
        if (response && response.profile_id !== snapshot.profileId) throw scopeChangedError();
    }

    function parseResponse(response) {
        if (typeof response?.has_saved_preferences !== 'boolean') throw new Error('Invalid preference response');
        const preferences = normalizeViewingPreferences(response.viewing_preferences);
        const legacyAutoplayOff = !response.has_saved_preferences && hasLegacyAutoplayOff(getStorage());
        return {
            hasSavedPreferences: response.has_saved_preferences,
            legacyAutoplayOff,
            preferences: Object.freeze({ ...preferences, autoplay_next_episode: legacyAutoplayOff ? false : preferences.autoplay_next_episode }),
        };
    }

    function load(expectedProfileId, context, options = { force: false }) {
        let scope;
        try {
            scope = normalizePreferenceScope(expectedProfileId, context, getDeviceId());
        } catch (error) {
            invalidate();
            publish({ ...emptyState(), profileId: expectedProfileId, status: 'error', error });
            return Promise.reject(error);
        }
        if (!options.force && loadFlight?.key === scope.key) return loadFlight.promise;
        if (saveFlight && snapshot.scope?.key === scope.key) return saveFlight.promise;
        if (!options.force && snapshot.scope?.key === scope.key && snapshot.preferences && ['ready', 'saving'].includes(snapshot.status)) {
            return Promise.resolve(snapshot);
        }
        invalidate();
        const capturedGeneration = generation;
        const controller = new AbortController();
        publish({ ...emptyState(), profileId: expectedProfileId, scope, status: 'loading' });
        const flight = { key: scope.key, controller, promise: null };
        flight.promise = Promise.resolve().then(async () => {
            try {
                const response = await getPreferences({ signal: controller.signal });
                requireCurrent(capturedGeneration, scope.key, response);
                const resolved = parseResponse(response);
                publish({ ...snapshot, ...resolved, devicePreferences: Object.freeze(readDevicePreferences(scope, getStorage())), status: 'ready', error: null });
                return snapshot;
            } catch (error) {
                requireCurrent(capturedGeneration, scope.key);
                publish({ ...snapshot, preferences: null, devicePreferences: null, status: 'error', error });
                throw error;
            } finally {
                if (loadFlight === flight) loadFlight = null;
            }
        });
        loadFlight = flight;
        return flight.promise;
    }

    function save(expectedProfileId, value) {
        if (snapshot.profileId !== expectedProfileId || !snapshot.scope || !snapshot.preferences) {
            return Promise.reject(scopeChangedError());
        }
        if (saveFlight) return Promise.reject(new Error('A preference save is already in progress'));
        let preferences;
        try {
            preferences = normalizeViewingPreferences(value);
        } catch (error) {
            return Promise.reject(error);
        }
        const capturedGeneration = generation;
        const key = snapshot.scope.key;
        const controller = new AbortController();
        publish({ ...snapshot, status: 'saving', error: null });
        const flight = { controller, promise: null };
        flight.promise = Promise.resolve().then(async () => {
            try {
                const response = await savePreferences(expectedProfileId, preferences, { signal: controller.signal });
                requireCurrent(capturedGeneration, key, response);
                if (response.has_saved_preferences !== true) throw new Error('Preference save was not confirmed');
                publish({ ...snapshot, ...parseResponse(response), status: 'ready', error: null });
                return snapshot;
            } catch (error) {
                requireCurrent(capturedGeneration, key);
                publish({ ...snapshot, status: 'ready', error });
                throw error;
            } finally {
                if (saveFlight === flight) saveFlight = null;
            }
        });
        saveFlight = flight;
        return flight.promise;
    }

    function saveDevice(expectedProfileId, value) {
        if (snapshot.profileId !== expectedProfileId || !snapshot.scope || !snapshot.preferences) throw scopeChangedError();
        const preferences = writeDevicePreferences(snapshot.scope, value, getStorage());
        publish({ ...snapshot, devicePreferences: Object.freeze(preferences) });
        return preferences;
    }

    return { subscribe: store.subscribe, load, save, saveDevice, invalidate, getSnapshot: () => snapshot };
}
