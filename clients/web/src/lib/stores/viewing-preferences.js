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
