/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function browserStorage() {
    try {
        return globalThis.localStorage || null;
    } catch {
        return null;
    }
}

export function hasLegacyAutoplayOff(storage) {
    try {
        const value = JSON.parse(storage?.getItem('duskcue_prefs') || 'null');
        return !!value && !Array.isArray(value) && typeof value === 'object' && value.autoplay === false;
    } catch {
        return false;
    }
}
