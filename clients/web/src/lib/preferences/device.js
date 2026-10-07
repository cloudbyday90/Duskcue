/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export const DEFAULT_DEVICE_PREFERENCES = Object.freeze({
    quality_mode: 'auto',
    max_streaming_bitrate: null,
});

function storageKey(scope) {
    if (!scope.serverOrigin || !scope.userId || !scope.deviceId) return null;
    return `duskcue_viewing_device_prefs_v1:${JSON.stringify([scope.serverOrigin, scope.userId, scope.deviceId])}`;
}

export function normalizeDevicePreferences(value) {
    if (!value || !['auto', 'maximum', 'manual'].includes(value.quality_mode)) {
        throw new Error('Invalid device quality preference');
    }
    const bitrate = value.quality_mode === 'manual' ? value.max_streaming_bitrate : null;
    if (value.quality_mode === 'manual' && (!Number.isSafeInteger(bitrate) || bitrate <= 0 || bitrate > 1000000000)) {
        throw new Error('Invalid device bitrate limit');
    }
    return { quality_mode: value.quality_mode, max_streaming_bitrate: bitrate };
}

export function readDevicePreferences(scope, storage) {
    try {
        const key = storageKey(scope);
        const stored = key && storage?.getItem(key);
        return stored ? normalizeDevicePreferences(JSON.parse(stored)) : { ...DEFAULT_DEVICE_PREFERENCES };
    } catch {
        return { ...DEFAULT_DEVICE_PREFERENCES };
    }
}

export function writeDevicePreferences(scope, value, storage) {
    const preferences = normalizeDevicePreferences(value);
    const key = storageKey(scope);
    if (!key || !storage) throw new Error('Device preferences could not be stored');
    storage.setItem(key, JSON.stringify(preferences));
    return preferences;
}

export function devicePreferencesEqual(first, second) {
    return !!first && !!second && first.quality_mode === second.quality_mode
        && first.max_streaming_bitrate === second.max_streaming_bitrate;
}
