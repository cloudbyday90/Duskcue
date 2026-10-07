import { test, expect, installDesktopApi } from './desktop-fixtures';
import { test as webTest } from './fixtures';

const savedServer = { origin: 'http://server.test:48027', network_mode: 'local' as const, display_name: null, last_connected_at: null };

test.describe('mocked desktop first-run server boundary', () => {
    test.use({ desktopOptions: { delayMs: 600 } });

    test('waits for explicit labeled connection before making any API request', async ({ page, api, desktop }) => {
        await page.goto('/dashboard');
        await expect(page.getByRole('heading', { name: 'Connect to your server', exact: true })).toBeVisible();
        expect(api.requests).toHaveLength(0);
        const url = page.getByLabel('Server URL', { exact: true });
        const mode = page.getByLabel('Network mode', { exact: true });
        await url.fill('server.test');
        await mode.selectOption('remote_vpn');
        await page.getByRole('button', { name: 'Test and connect', exact: true }).focus();
        await page.keyboard.press('Enter');
        await expect(page.getByText('Testing the connection and saving the server.', { exact: true })).toBeVisible();
        await expect(url).toBeDisabled();
        await expect(mode).toBeDisabled();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
        expect(desktop.server).toMatchObject({ origin: savedServer.origin, network_mode: 'remote_vpn' });
        const commands = desktop.calls.filter((call) => ['test_server_connection', 'save_server_connection', 'read_session_token'].includes(call.command)).map((call) => call.command);
        expect(commands).toEqual(['test_server_connection', 'read_session_token', 'save_server_connection']);
        expect(api.requests.find((request) => request.path === '/setup/status')).toBeDefined();
        expect(desktop.calls.filter((call) => call.command === 'write_session_token')).toHaveLength(0);
    });

    test('shows native normalization failure and keeps draft and focus for correction', async ({ page, api, desktop }) => {
        await page.goto('/dashboard');
        const url = page.getByLabel('Server URL', { exact: true });
        await url.fill('bad value');
        await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
        const error = page.getByRole('alert');
        await expect(error).toHaveText('Enter a valid http(s) server URL.');
        await expect(error).toBeFocused();
        await expect(url).toHaveValue('bad value');
        expect(api.requests).toHaveLength(0);
        expect(desktop.calls.filter((call) => call.command === 'save_server_connection')).toHaveLength(0);
        await url.fill('server.test');
        await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
        expect(desktop.server.origin).toBe(savedServer.origin);
    });
});

test.describe('mocked desktop settings switch', () => {
    test.use({ desktopOptions: { server: savedServer, token: 'fixture-desktop-token', delayMs: 300 } });

    test('exposes desktop Settings entry and retains failed switching form before successful retry', async ({ page, api, desktop }) => {
        await page.goto('/settings');
        await page.getByRole('link', { name: /Server connection/ }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/settings/server');
        const url = page.getByLabel('Server URL', { exact: true });
        await expect(url).toHaveValue(savedServer.origin);
        desktop.failures.set('test_server_connection', 'Fixture desktop server offline');
        await url.fill('next.test');
        await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
        await expect(page.getByRole('alert')).toHaveText('Fixture desktop server offline');
        await expect(page.getByRole('alert')).toBeFocused();
        await expect(url).toHaveValue('next.test');
        expect(desktop.server.origin).toBe(savedServer.origin);
        expect(desktop.calls.filter((call) => call.command === 'save_server_connection')).toHaveLength(0);
        desktop.failures.delete('test_server_connection');
        await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
        expect(desktop.server.origin).toBe('http://next.test:48027');
        expect(desktop.tokens.get(savedServer.origin)).toBe('fixture-desktop-token');
        expect(api.requests.filter((request) => request.path === '/auth/logout')).toHaveLength(0);
    });

    test('native unsaved dialog keeps draft on Escape and restores the invoking navigation focus', async ({ page, desktop }) => {
        await page.goto('/settings/server');
        const url = page.getByLabel('Server URL', { exact: true });
        await expect(url).toHaveValue(savedServer.origin);
        await url.fill('next.test');
        const home = page.getByRole('link', { name: 'Home', exact: true }).first();
        await home.click();
        const dialog = page.getByRole('dialog', { name: 'Discard server changes?', exact: true });
        await expect(dialog).toBeVisible();
        await expect(dialog.getByRole('button', { name: 'Keep editing', exact: true })).toBeFocused();
        await page.keyboard.press('Escape');
        await expect(dialog).not.toBeVisible();
        await expect(home).toBeFocused();
        await expect(url).toHaveValue('next.test');
        expect(new URL(page.url()).pathname).toBe('/settings/server');
        await page.getByRole('button', { name: 'Discard changes', exact: true }).click();
        await expect(url).toHaveValue(savedServer.origin);
        await expect(url).toBeFocused();
        await url.fill('next.test');
        await home.click();
        await dialog.getByRole('button', { name: 'Discard changes', exact: true }).click();
        await expect.poll(() => new URL(page.url()).pathname).toBe('/dashboard');
        expect(desktop.calls.filter((call) => call.command === 'save_server_connection')).toHaveLength(0);
    });
});

webTest('ordinary browser server route explains hosting and keeps desktop choice out of Settings', async ({ page, api }) => {
    await installDesktopApi(page, api);
    await page.goto('/settings');
    await expect(page.getByRole('link', { name: /Server connection/ })).toHaveCount(0);
    await page.goto('/settings/server');
    await expect(page.getByRole('heading', { name: 'Server connection', exact: true })).toBeVisible();
    await expect(page.getByText('The browser uses the server hosting this website. Choose a different server in the Duskcue desktop app.', { exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Test and connect', exact: true })).toHaveCount(0);
    expect(api.unhandledRequests).toEqual([]);
});
