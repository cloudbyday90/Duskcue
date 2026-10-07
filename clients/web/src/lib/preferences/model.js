/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export const DEFAULT_VIEWING_PREFERENCES = Object.freeze({
    autoplay_next_episode: true,
    audio_language: null,
    prefer_audio_description: false,
    subtitle_mode: 'none',
    subtitle_language: null,
    prefer_sdh: false,
});

const LANGUAGE_ALIASES = new Map([
    ['ara', 'ar'], ['ben', 'bn'], ['bul', 'bg'], ['cat', 'ca'], ['ces', 'cs'], ['cze', 'cs'],
    ['chi', 'zh'], ['zho', 'zh'], ['dan', 'da'], ['deu', 'de'], ['ger', 'de'], ['dut', 'nl'],
    ['nld', 'nl'], ['ell', 'el'], ['gre', 'el'], ['eng', 'en'], ['est', 'et'], ['fas', 'fa'],
    ['per', 'fa'], ['fin', 'fi'], ['fra', 'fr'], ['fre', 'fr'], ['heb', 'he'], ['hin', 'hi'],
    ['hrv', 'hr'], ['hun', 'hu'], ['ice', 'is'], ['isl', 'is'], ['ind', 'id'], ['ita', 'it'],
    ['jpn', 'ja'], ['kor', 'ko'], ['lav', 'lv'], ['lit', 'lt'], ['may', 'ms'], ['msa', 'ms'],
    ['nob', 'nb'], ['nno', 'nn'], ['nor', 'no'], ['pol', 'pl'], ['por', 'pt'], ['ron', 'ro'],
    ['rum', 'ro'], ['rus', 'ru'], ['slk', 'sk'], ['slo', 'sk'], ['slv', 'sl'], ['spa', 'es'],
    ['srp', 'sr'], ['swe', 'sv'], ['tam', 'ta'], ['tel', 'te'], ['tha', 'th'], ['tur', 'tr'],
    ['ukr', 'uk'], ['urd', 'ur'], ['vie', 'vi'],
]);

export const PREFERENCE_LANGUAGE_CODES = [
    'ar', 'bn', 'bg', 'zh', 'hr', 'cs', 'da', 'nl', 'en', 'et', 'fi', 'fr', 'de', 'el',
    'he', 'hi', 'hu', 'is', 'id', 'it', 'ja', 'ko', 'lv', 'lt', 'ms', 'nb', 'nn', 'no',
    'fa', 'pl', 'pt', 'ro', 'ru', 'sr', 'sk', 'sl', 'es', 'sv', 'ta', 'te', 'th', 'tr',
    'uk', 'ur', 'vi',
];

export function canonicalLanguage(value) {
    if (value === null || value === undefined) return null;
    if (typeof value !== 'string' || value.length > 32) throw new Error('Invalid language code');
    const code = value.trim().toLowerCase();
    if (!/^[a-z]{2,3}$/.test(code) || ['und', 'mul', 'zxx'].includes(code)) {
        throw new Error('Invalid language code');
    }
    return LANGUAGE_ALIASES.get(code) || code;
}

export function normalizeViewingPreferences(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) {
        throw new Error('Invalid viewing preferences');
    }
    for (const key of ['autoplay_next_episode', 'prefer_audio_description', 'prefer_sdh']) {
        if (typeof value[key] !== 'boolean') throw new Error('Invalid viewing preferences');
    }
    if (!['none', 'always'].includes(value.subtitle_mode)) throw new Error('Invalid subtitle mode');
    const preferences = {
        autoplay_next_episode: value.autoplay_next_episode,
        audio_language: canonicalLanguage(value.audio_language),
        prefer_audio_description: value.prefer_audio_description,
        subtitle_mode: value.subtitle_mode,
        subtitle_language: canonicalLanguage(value.subtitle_language),
        prefer_sdh: value.prefer_sdh,
    };
    if (preferences.subtitle_mode === 'always' && !preferences.subtitle_language) {
        throw new Error('A subtitle language is required');
    }
    return preferences;
}

export function preferencesEqual(first, second) {
    if (!first || !second) return first === second;
    return Object.keys(DEFAULT_VIEWING_PREFERENCES).every((key) => first[key] === second[key]);
}

export function normalizePreferenceScope(profileId, context, deviceId = null) {
    const origin = new URL(context.serverOrigin || globalThis.location?.origin).origin;
    if (!/^https?:\/\//.test(origin) || !profileId || !context.userId) {
        throw new Error('A confirmed profile scope is required');
    }
    const scope = { profileId, userId: context.userId, serverOrigin: origin, deviceId: context.deviceId || deviceId };
    return { ...scope, key: JSON.stringify([origin, scope.userId, profileId, scope.deviceId]) };
}

export function scopeChangedError() {
    return new DOMException('The active profile changed', 'AbortError');
}
