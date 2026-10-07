/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { getViewingPreferences, saveViewingPreferences } from '../api/profiles.js';
import { getDeviceIdentity } from '../device/identity.js';
import { createViewingPreferencesStore } from '../preferences/store.js';

export { createViewingPreferencesStore } from '../preferences/store.js';

export const viewingPreferences = createViewingPreferencesStore({
    getPreferences: getViewingPreferences,
    savePreferences: saveViewingPreferences,
    getDeviceId() {
        try {
            return getDeviceIdentity();
        } catch {
            return null;
        }
    },
});
