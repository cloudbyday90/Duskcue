import { readFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';

export async function assertNativeIsolation(page, manifest) {
    expect(await invokeNative(page, 'plugin:app|identifier')).toBe(manifest.identifier);
    const appData = await invokeNative(page, 'plugin:path|resolve_directory', { directory: 14 });
    expect(resolve(appData).toLowerCase()).toBe(resolve(manifest.expectedAppData).toLowerCase());
    const initial = await invokeNative(page, 'read_server_connections');
    expect(initial).toEqual({ last_server: null, saved_servers: [] });
    for (const origin of manifest.origins) {
        expect(await invokeNative(page, 'read_session_token', { req: { server_origin: origin } })).toBeNull();
    }
    return { identifier: manifest.identifier, appData, appInfo: await invokeNative(page, 'app_info'), tauriVersion: await invokeNative(page, 'plugin:app|tauri_version') };
}

export async function loginFixture(page, api) {
    await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
    await page.getByRole('button', { name: 'Password', exact: true }).click();
    await page.getByLabel('Username', { exact: true }).fill(api.credentials.username);
    await page.getByLabel('Password', { exact: true }).fill(api.credentials.password);
    await page.getByRole('button', { name: 'Sign In', exact: true }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
}

export async function exerciseConnection(page, manifest, api, health, screenshot) {
    await expect(page.getByRole('heading', { name: 'Connect to your server', exact: true })).toBeVisible();
    expect(api.scenario.requests).toHaveLength(0);
    const url = page.getByLabel('Server URL', { exact: true });
    await url.fill(`${new URL(manifest.origins[0]).hostname}:48028`);
    await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
    await expect(page.getByRole('alert')).toHaveText('Use the public Duskcue port 48027, not the internal API port 48028.');
    await expect(page.getByRole('alert')).toBeFocused();
    expect(api.scenario.requests).toHaveLength(0);
    expect(health.requests).toHaveLength(0);
    await screenshot('native-connection-error');
    await url.fill(new URL(manifest.origins[0]).hostname);
    await page.getByLabel('Network mode', { exact: true }).selectOption('local');
    await page.getByRole('button', { name: 'Test and connect', exact: true }).focus();
    await page.keyboard.press('Enter');
    await loginFixture(page, api);
    expect(health.requests).toContainEqual({ origin: manifest.origins[0], method: 'GET', path: '/health/ready' });
    const saved = await invokeNative(page, 'read_server_connections');
    expect(saved.last_server).toMatchObject({ origin: manifest.origins[0], network_mode: 'local' });
    const file = JSON.parse(await readFile(join(manifest.expectedAppData, 'server-connections.json'), 'utf8'));
    expect(file).toEqual(saved);
    expect(await invokeNative(page, 'read_session_token', { req: { server_origin: manifest.origins[0] } })).toBe(api.tokens.get(manifest.origins[0]));
    expect(api.authRequests[0]).toMatchObject({ clientPlatform: 'desktop', deviceIdPresent: true });
    const secretsAbsent = await page.evaluate((token) => Object.values(localStorage).every((value) => !value.includes(token)), api.tokens.get(manifest.origins[0]));
    expect(secretsAbsent).toBe(true);
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    expect(await page.evaluate(() => localStorage.getItem('duskcue_device_id'))).toBe(api.authRequests[0].deviceId);
    const sessionRequests = api.transportRequests.filter((entry) => entry.path === '/user/sessions');
    expect(sessionRequests.length).toBeGreaterThan(0);
    expect(sessionRequests.every((entry) => entry.authenticated && entry.origin === manifest.origins[0])).toBe(true);
    return { realHealth: true, savedNativeFile: true, realKeyringRoundTrip: true, restoredAfterReload: true };
}

export async function exerciseServerSwitch(page, manifest, api, health) {
    const marker = `native-switch-${Date.now()}`;
    await page.evaluate((value) => { document.documentElement.dataset.nativeSwitchDocument = value; }, marker);
    await expect.poll(() => api.events.pendingCount(manifest.origins[0])).toBe(1);
    const originalEvents = api.events.pendingRecords(manifest.origins[0]);
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('link', { name: 'Settings', exact: true }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe('/settings');
    await page.getByRole('link', { name: /Server connection/ }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe('/settings/server');
    const input = page.getByLabel('Server URL', { exact: true });
    await expect(input).toHaveValue(manifest.origins[0]);
    await input.fill(manifest.origins[1]);
    await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
    expect((await invokeNative(page, 'read_server_connections')).last_server.origin).toBe(manifest.origins[1]);
    expect(await invokeNative(page, 'read_session_token', { req: { server_origin: manifest.origins[0] } })).toBe(api.tokens.get(manifest.origins[0]));
    expect(await invokeNative(page, 'read_session_token', { req: { server_origin: manifest.origins[1] } })).toBeNull();
    await loginFixture(page, api);
    expect(api.authRequests.at(-1).deviceId).toBe(api.authRequests[0].deviceId);
    expect(await page.evaluate(() => document.documentElement.dataset.nativeSwitchDocument)).toBe(marker);
    await expect.poll(() => api.events.pendingCount(manifest.origins[0])).toBe(0);
    for (const request of originalEvents) expect(request.aborted).toBe(true);
    await expect.poll(() => api.events.pendingCount(manifest.origins[1])).toBe(1);
    expect(await invokeNative(page, 'read_session_token', { req: { server_origin: manifest.origins[1] } })).toBe(api.tokens.get(manifest.origins[1]));
    expect(health.requests).toContainEqual({ origin: manifest.origins[1], method: 'GET', path: '/health/ready' });
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('link', { name: 'Viewing preferences', exact: true }).click();
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).toBeChecked();
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('fr');
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('auto');
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.dataset.nativeSwitchDocument)).toBe(marker);
    return { selectedSecondOrigin: true, originalCredentialRetained: true, newCredentialScoped: true, sameDocumentSwitch: true, originalSseRequestAborted: true, newSseRequestPending: true, newServerDefaultsAndDeviceQualityIsolated: true, documentMarker: marker };
}

export async function clearFixtureCredentials(page, manifest, api) {
    const cleared = [];
    for (const origin of manifest.origins) {
        const current = await invokeNative(page, 'read_session_token', { req: { server_origin: origin } });
        if (current === null) continue;
        if (current !== api.tokens.get(origin)) throw new Error('A qualification origin contains an unexpected credential; it was left untouched.');
        await invokeNative(page, 'clear_session_token', { req: { server_origin: origin } });
        expect(await invokeNative(page, 'read_session_token', { req: { server_origin: origin } })).toBeNull();
        cleared.push(origin);
    }
    return cleared;
}
