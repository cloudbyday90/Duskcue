import { get } from 'svelte/store';
import { auth as sharedAuth } from '../stores/auth.js';
import { getServerOrigin } from '../api/core.js';
import { playbackContextKey } from './release.js';

export function createPlaybackAuthorizationRecovery({ auth = null, readContext = null } = {}) {
    auth ||= sharedAuth;
    if (!readContext && auth !== sharedAuth) throw new TypeError('An injected authorization facade requires its context reader.');
    readContext ||= () => ({ serverOrigin: getServerOrigin() || globalThis.location?.origin, userId: get(sharedAuth).user?.id });
    let revision = 0;
    let disposed = false;

    async function recover(error, expectedContext) {
        const token = revision;
        const key = playbackContextKey(expectedContext);
        if (disposed || error?.status !== 401 || !key || key !== playbackContextKey(readContext())) return { status: 'superseded' };
        const outcome = await auth.checkSession({ revalidate: true, expectedContext });
        if (disposed || token !== revision || !outcome || typeof outcome !== 'object' || !outcome.isCurrent?.()) return { status: 'superseded' };
        const actual = readContext();
        if (outcome.status === 'expired') {
            let origin;
            try { origin = new URL(actual.serverOrigin).origin; } catch {}
            if (actual.userId || origin !== new URL(expectedContext.serverOrigin).origin) return { status: 'superseded' };
        } else if (key !== playbackContextKey(actual)) return { status: 'superseded' };
        return outcome;
    }

    return { recover, dispose() { disposed = true; revision += 1; } };
}
