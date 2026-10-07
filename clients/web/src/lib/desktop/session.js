import { setServerOrigin, clearServerOrigin, setBearerToken, clearBearerToken } from '../api/core.js';
import { isTauriDesktop } from './tauri.js';

async function invokeNative(command, args = {}) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke(command, args);
}

export function createDesktopSession({ isDesktop = isTauriDesktop, invoke = invokeNative, setOrigin = setServerOrigin, clearOrigin = clearServerOrigin, setToken = setBearerToken, clearToken = clearBearerToken } = {}) {
    let server = null;
    let hasToken = false;
    let initialized = false;
    let initializing = null;
    let queue = Promise.resolve();

    function snapshot() {
        const desktop = isDesktop();
        return { isDesktop: desktop, server: server ? { ...server } : null, requiresServerSelection: desktop && !server, hasToken };
    }

    function serialize(action) {
        const pending = queue.then(() => action());
        queue = pending.catch(() => {});
        return pending;
    }

    async function prepare(selected) {
        if (!selected?.origin) return { server: null, token: null };
        const mode = selected.network_mode || 'local';
        const origin = await invoke('normalize_server_origin', { input: selected.origin, networkMode: mode });
        const token = await invoke('read_session_token', { req: { server_origin: origin } });
        return { server: { ...selected, origin, network_mode: mode }, token: typeof token === 'string' && token.trim() ? token : null };
    }

    function commit(prepared) {
        clearToken();
        hasToken = false;
        server = prepared.server;
        if (server) setOrigin(server.origin);
        else clearOrigin();
        if (prepared.token) {
            setToken(prepared.token);
            hasToken = true;
        }
        return snapshot();
    }

    function initialize() {
        if (!isDesktop() || initialized) return Promise.resolve(snapshot());
        if (initializing) return initializing;
        initializing = serialize(async () => {
            try {
                const saved = await invoke('read_server_connections');
                const result = commit(await prepare(saved?.last_server));
                initialized = true;
                return result;
            } catch (error) {
                clearToken();
                hasToken = false;
                throw error;
            }
        }).finally(() => { initializing = null; });
        return initializing;
    }

    async function select({ input, networkMode = 'local' }) {
        if (!isDesktop()) throw new Error('Server selection is available in the desktop app.');
        if (!['local', 'remote_vpn', 'exposed'].includes(networkMode)) throw new Error('Choose a valid network mode.');
        return serialize(async () => {
            const origin = await invoke('normalize_server_origin', { input, networkMode });
            const probe = await invoke('test_server_connection', { input: origin, networkMode });
            if (!probe?.healthy) throw new Error('The selected server is not ready.');
            const prepared = await prepare({ origin, network_mode: networkMode });
            const saved = await invoke('save_server_connection', { profile: { origin, network_mode: networkMode } });
            if (saved?.last_server?.origin !== prepared.server.origin) throw new Error('The selected server could not be saved.');
            prepared.server = { ...saved.last_server, ...prepared.server };
            const result = commit(prepared);
            initialized = true;
            return result;
        });
    }

    async function persist(token, expectedOrigin = null) {
        if (!isDesktop()) return snapshot();
        if (typeof token !== 'string' || !token.trim()) throw new Error('The server did not return a session token.');
        await initialize();
        return serialize(async () => {
            if (!server || (expectedOrigin && server.origin !== expectedOrigin)) throw new DOMException('The selected server changed.', 'AbortError');
            await invoke('write_session_token', { req: { server_origin: server.origin, token } });
            setToken(token);
            hasToken = true;
            return snapshot();
        });
    }

    async function clear(expectedOrigin = null) {
        if (!isDesktop()) return snapshot();
        await initialize();
        return serialize(async () => {
            if (expectedOrigin && server?.origin !== expectedOrigin) throw new DOMException('The selected server changed.', 'AbortError');
            clearToken();
            hasToken = false;
            if (server) await invoke('clear_session_token', { req: { server_origin: server.origin } });
            return snapshot();
        });
    }

    return { initialize, select, persist, clear, getScope: snapshot };
}

const desktopSession = createDesktopSession();

export const initializeDesktopSession = desktopSession.initialize;
export const selectDesktopServer = desktopSession.select;
export const persistDesktopSession = desktopSession.persist;
export const clearDesktopSession = desktopSession.clear;
export const getDesktopSessionScope = desktopSession.getScope;
