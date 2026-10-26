/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { instant } from './instant.js';

export function textLanguage(raw) {
    if (typeof raw !== 'string' || raw.length > 64 || !/^[a-zA-Z]{2,8}(?:-[a-zA-Z0-9]{1,8})*$/.test(raw)) return '';
    try { return Intl.getCanonicalLocales(raw)[0] || ''; } catch { return ''; }
}

export function preferredText(variants, locale) {
    const language = textLanguage(locale).toLowerCase();
    return variants.find((entry) => language && textLanguage(entry.language).toLowerCase() === language)
        || variants.find((entry) => language && textLanguage(entry.language).split('-')[0].toLowerCase() === language.split('-')[0])
        || variants.find((entry) => !entry.language)
        || variants[0] || null;
}

export function formatInstant(value, locale) {
    if (!value) return '';
    try {
        return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(instant(value).milliseconds);
    } catch { return value; }
}

export function deviceTimeZone() {
    try { return Intl.DateTimeFormat().resolvedOptions().timeZone || ''; } catch { return ''; }
}
