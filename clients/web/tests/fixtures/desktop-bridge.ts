import type { Page } from '@playwright/test';

export type DesktopServer = { origin: string; network_mode: 'local' | 'remote_vpn' | 'exposed'; display_name?: string | null; last_connected_at?: string | null };
export type DesktopOptions = { server?: DesktopServer | null; token?: string | null; delayMs?: number };

export async function installDesktopBridge(page: Page, user: object, options: DesktopOptions = {}) {
    const state = {
        server: options.server || null,
        calls: [] as Array<{ command: string; args: Record<string, any> }>,
        failures: new Map<string, string>(),
        tokens: new Map<string, string>(),
        unhandled: [] as string[],
        fullscreen: false,
    };
    if (state.server && options.token) state.tokens.set(state.server.origin, options.token);
    let eventId = 0;

    await page.exposeBinding('desktopTestInvoke', async (_source, command: string, args: Record<string, any> = {}) => {
        state.calls.push({ command, args });
        if (state.failures.has(command)) throw new Error(state.failures.get(command));
        if (command === 'read_server_connections') return { last_server: state.server, saved_servers: state.server ? [state.server] : [] };
        if (command === 'normalize_server_origin') {
            const allowed = new Map([
                ['server.test', 'http://server.test:48027'],
                ['next.test', 'http://next.test:48027'],
                ['https://next.test', 'https://next.test:48027'],
            ]);
            if (args.input === state.server?.origin) return args.input;
            const value = allowed.get(args.input) || [...allowed.values()].find((origin) => origin === args.input);
            if (!value) throw new Error('Enter a valid http(s) server URL.');
            return value;
        }
        if (command === 'test_server_connection') {
            if (options.delayMs) await new Promise((resolve) => setTimeout(resolve, options.delayMs));
            return { origin: args.input, status: 200, healthy: true, body: null };
        }
        if (command === 'save_server_connection') {
            state.server = { ...args.profile, display_name: null, last_connected_at: null };
            return { last_server: state.server, saved_servers: [state.server] };
        }
        if (command === 'read_session_token') return state.tokens.get(args.req.server_origin) || null;
        if (command === 'write_session_token') { state.tokens.set(args.req.server_origin, args.req.token); return null; }
        if (command === 'clear_session_token') { state.tokens.delete(args.req.server_origin); return null; }
        if (command === 'plugin:window|is_fullscreen') return state.fullscreen;
        if (command === 'plugin:window|set_fullscreen') { state.fullscreen = args.value === true; return null; }
        if (command === 'plugin:event|listen') return ++eventId;
        if (command === 'plugin:event|unlisten' || command === 'show_native_notification') return null;
        state.unhandled.push(command);
        throw new Error(`Unhandled desktop test command: ${command}`);
    });

    await page.addInitScript(({ server, user }) => {
        const testWindow = window as typeof window & {
            desktopTestInvoke: (command: string, args: object) => Promise<unknown>;
            __TAURI_INTERNALS__: object;
            __TAURI_EVENT_PLUGIN_INTERNALS__: object;
        };
        let callbackId = 0;
        testWindow.__TAURI_INTERNALS__ = {
            metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
            invoke: (command: string, args: object = {}) => testWindow.desktopTestInvoke(command, args),
            transformCallback: () => ++callbackId,
            unregisterCallback: () => {},
        };
        testWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
        if (server) localStorage.setItem(`duskcue_user:${encodeURIComponent(server.origin)}`, JSON.stringify(user));
    }, { server: state.server, user });

    return state;
}
