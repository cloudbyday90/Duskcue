const CHANNEL_NAME = 'duskcue-profile-scope';
const STORAGE_KEY = 'duskcue-profile-scope-event';

let channel = null;

export function startProfileScopeSync(onChange) {
    if (typeof window === 'undefined') return () => {};

    const receive = (payload) => {
        if (!isProfileScopeEvent(payload)) return;
        onChange(payload);
    };
    const onStorage = (event) => {
        if (event.key !== STORAGE_KEY || !event.newValue) return;
        try {
            receive(JSON.parse(event.newValue));
        } catch {
        }
    };

    if (typeof BroadcastChannel !== 'undefined') {
        channel = new BroadcastChannel(CHANNEL_NAME);
        channel.addEventListener('message', (event) => receive(event.data));
    }
    window.addEventListener('storage', onStorage);

    return () => {
        channel?.close();
        channel = null;
        window.removeEventListener('storage', onStorage);
    };
}

export function publishProfileScopeChange({ userId, profileId }) {
    if (typeof window === 'undefined') return;
    const payload = {
        type: 'profile-scope-changed',
        user_id: userId,
        profile_id: profileId,
        revision: createRevision(),
    };
    channel?.postMessage(payload);
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(payload));
        localStorage.removeItem(STORAGE_KEY);
    } catch {
    }
}

function createRevision() {
    return globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random()}`;
}

function isProfileScopeEvent(value) {
    return value
        && value.type === 'profile-scope-changed'
        && typeof value.user_id === 'string'
        && typeof value.profile_id === 'string'
        && typeof value.revision === 'string';
}
