import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDesktopSession } from '../../src/lib/desktop/session.js';
import { buildApiUrl, clearBearerToken, clearServerOrigin, getServerOrigin, mediaRequestHeaders } from '../../src/lib/api/core.js';

vi.mock('../../src/lib/desktop/tauri.js', () => ({ isTauriDesktop: () => false }));

const firstServer = { origin: 'https://first.invalid:48027', network_mode: 'exposed' };
const secondServer = { origin: 'http://second.invalid:48027', network_mode: 'local' };

function deferred<T = any>() {
    let resolve!: (value: T) => void;
    const promise = new Promise<T>((settle) => { resolve = settle; });
    return { promise, resolve };
}

function nativeFixture(selected: any = firstServer) {
    let saved = selected;
    const tokens = new Map([[firstServer.origin, 'first-native-secret'], [secondServer.origin, 'second-native-secret']]);
    const invoke = vi.fn(async (command: string, args: any = {}) => {
        if (command === 'read_server_connections') return { last_server: saved, saved_servers: saved ? [saved] : [] };
        if (command === 'normalize_server_origin') return args.input;
        if (command === 'read_session_token') return tokens.get(args.req.server_origin) ?? null;
        if (command === 'write_session_token') { tokens.set(args.req.server_origin, args.req.token); return; }
        if (command === 'clear_session_token') { tokens.delete(args.req.server_origin); return; }
        if (command === 'test_server_connection') return { origin: args.input, status: 200, healthy: true };
        if (command === 'save_server_connection') { saved = args.profile; return { last_server: saved, saved_servers: [saved] }; }
        throw new Error('Unknown test command');
    });
    const service = createDesktopSession({ isDesktop: () => true, invoke });
    return { service, invoke, tokens };
}

afterEach(() => { clearBearerToken(); clearServerOrigin(); });

describe('desktop native session boundary', () => {
    it('deduplicates bootstrap and configures selected-origin API and media credentials without returning secrets', async () => {
        const { service, invoke } = nativeFixture();
        const first = service.initialize();
        expect(service.initialize()).toBe(first);
        const result = await first;
        expect(invoke.mock.calls.map(([command]) => command)).toEqual(['read_server_connections', 'normalize_server_origin', 'read_session_token']);
        expect(result).toEqual({ isDesktop: true, server: firstServer, requiresServerSelection: false, hasToken: true });
        expect(JSON.stringify(result)).not.toContain('secret');
        expect(buildApiUrl('/setup/status')).toBe(`${firstServer.origin}/api/v1/setup/status`);
        expect(mediaRequestHeaders(`${firstServer.origin}/api/v1/transcode/stream/manifest.m3u8`)).toEqual({ Authorization: 'Bearer first-native-secret' });
        expect(() => mediaRequestHeaders(`${secondServer.origin}/api/v1/transcode/stream/manifest.m3u8`)).toThrow();
    });

    it('leaves ordinary browser cookie authentication untouched', async () => {
        const invoke = vi.fn();
        const service = createDesktopSession({ isDesktop: () => false, invoke });
        expect(await service.initialize()).toEqual({ isDesktop: false, server: null, requiresServerSelection: false, hasToken: false });
        await service.persist('browser-response-token');
        await service.clear();
        expect(invoke).not.toHaveBeenCalled();
        expect(getServerOrigin()).toBeNull();
    });

    it('gates first run when no server has actually been selected', async () => {
        const { service, invoke } = nativeFixture(null);
        expect(await service.initialize()).toMatchObject({ requiresServerSelection: true, hasToken: false });
        expect(invoke).toHaveBeenCalledTimes(1);
        expect(getServerOrigin()).toBeNull();
    });

    it('tests readiness before saving and loads only the selected server token', async () => {
        const { service, invoke } = nativeFixture();
        await service.initialize();
        invoke.mockClear();
        const result = await service.select({ input: secondServer.origin, networkMode: 'local' });
        expect(invoke.mock.calls.map(([command]) => command)).toEqual(['normalize_server_origin', 'test_server_connection', 'normalize_server_origin', 'read_session_token', 'save_server_connection']);
        expect(result.server).toEqual(secondServer);
        expect(mediaRequestHeaders(`${secondServer.origin}/api/v1/stream/file`)).toEqual({ Authorization: 'Bearer second-native-secret' });
    });

    it('does not save or adopt an unhealthy server', async () => {
        const { service, invoke } = nativeFixture();
        await service.initialize();
        const base = invoke.getMockImplementation()!;
        invoke.mockImplementation(async (command, args) => command === 'test_server_connection' ? { healthy: false } : base(command, args));
        await expect(service.select({ input: secondServer.origin })).rejects.toThrow('not ready');
        expect(invoke.mock.calls.some(([command]) => command === 'save_server_connection')).toBe(false);
        expect(service.getScope().server?.origin).toBe(firstServer.origin);
    });

    it('preserves the active origin and token if another server keyring read fails', async () => {
        const { service, invoke } = nativeFixture();
        await service.initialize();
        invoke.mockClear();
        const base = invoke.getMockImplementation()!;
        invoke.mockImplementation(async (command, args) => {
            if (command === 'read_session_token') throw new Error('OS keyring is locked');
            return base(command, args);
        });
        await expect(service.select({ input: secondServer.origin })).rejects.toThrow('locked');
        expect(invoke.mock.calls.some(([command]) => command === 'save_server_connection')).toBe(false);
        expect(service.getScope()).toMatchObject({ server: firstServer, hasToken: true });
        expect(getServerOrigin()).toBe(firstServer.origin);
        expect(mediaRequestHeaders(`${firstServer.origin}/api/v1/stream/file`)).toEqual({ Authorization: 'Bearer first-native-secret' });
        expect(await service.initialize()).toMatchObject({ server: firstServer, hasToken: true });
    });

    it('writes and clears only the existing origin-keyed OS credential', async () => {
        const { service, tokens } = nativeFixture();
        await service.initialize();
        await service.persist('replacement-secret', firstServer.origin);
        expect(tokens.get(firstServer.origin)).toBe('replacement-secret');
        expect(mediaRequestHeaders(`${firstServer.origin}/api/v1/stream/file`)).toEqual({ Authorization: 'Bearer replacement-secret' });
        await service.clear(firstServer.origin);
        expect(tokens.has(firstServer.origin)).toBe(false);
        expect(tokens.get(secondServer.origin)).toBe('second-native-secret');
        expect(service.getScope().hasToken).toBe(false);
        expect(mediaRequestHeaders(`${firstServer.origin}/api/v1/stream/file`)).toEqual({});
    });

    it('rejects a late login credential or logout cleanup after server selection changes', async () => {
        const { service, tokens } = nativeFixture();
        await service.initialize();
        await service.select({ input: secondServer.origin });
        await expect(service.persist('late-secret', firstServer.origin)).rejects.toMatchObject({ name: 'AbortError' });
        await expect(service.clear(firstServer.origin)).rejects.toMatchObject({ name: 'AbortError' });
        expect(tokens.get(secondServer.origin)).toBe('second-native-secret');
    });

    it('serializes logout behind an already pending keyring write', async () => {
        const { service, invoke, tokens } = nativeFixture();
        await service.initialize();
        const writing = deferred();
        const base = invoke.getMockImplementation()!;
        invoke.mockImplementation(async (command, args) => {
            if (command === 'write_session_token') await writing.promise;
            return base(command, args);
        });
        const write = service.persist('pending-secret', firstServer.origin);
        const clear = service.clear(firstServer.origin);
        await Promise.resolve();
        expect(tokens.has(firstServer.origin)).toBe(true);
        writing.resolve(undefined);
        await Promise.all([write, clear]);
        expect(tokens.has(firstServer.origin)).toBe(false);
        expect(service.getScope().hasToken).toBe(false);
    });

    it('propagates keyring failures without silently replacing credentials', async () => {
        const { service, invoke } = nativeFixture();
        await service.initialize();
        const base = invoke.getMockImplementation()!;
        invoke.mockImplementation(async (command, args) => {
            if (command === 'write_session_token') throw new Error('OS keyring is locked');
            return base(command, args);
        });
        await expect(service.persist('unsaved-secret', firstServer.origin)).rejects.toThrow('locked');
        expect(mediaRequestHeaders(`${firstServer.origin}/api/v1/stream/file`)).toEqual({ Authorization: 'Bearer first-native-secret' });
    });
});
