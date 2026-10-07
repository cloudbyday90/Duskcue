import { initializeDesktopSession, getDesktopSessionScope } from './session.js';
import { getSetupStatus } from '../api/auth.js';

export function createShellStartup({ auth, onChange, initializeSession = initializeDesktopSession, sessionScope = getDesktopSessionScope, readSetup = getSetupStatus }) {
    let generation = 0;
    let disposed = false;
    let state = { ready: false, desktop: null, setupRequired: false, error: null };
    function publish(update) { state = { ...state, ...update }; if (!disposed) onChange(state); }

    async function start() {
        const token = ++generation;
        const current = () => !disposed && generation === token;
        publish({ ready: false, error: null });
        try {
            const desktop = await initializeSession();
            if (!current()) return null;
            publish({ desktop });
            if (desktop.requiresServerSelection) return state;
            auth.init();
            if (desktop.isDesktop) await auth.checkSession();
            if (!current()) return null;
            let setupRequired = false;
            try { setupRequired = !!(await readSetup())?.setup_required; } catch {}
            if (!current()) return null;
            publish({ ready: true, setupRequired });
            return state;
        } catch (error) {
            if (current()) publish({ desktop: sessionScope(), error });
            return current() ? state : null;
        }
    }

    return { start, dispose() { disposed = true; generation += 1; } };
}
