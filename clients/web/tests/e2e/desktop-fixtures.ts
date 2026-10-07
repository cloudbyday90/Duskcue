import { test as base, expect } from './fixtures';
import { installDesktopBridge, type DesktopOptions } from '../fixtures/desktop-bridge';
import type { Page } from '@playwright/test';
import { createApiScenario } from '../fixtures/api.js';

export async function installDesktopApi(page: Page, api: ReturnType<typeof createApiScenario>) {
    await page.route(/\/api\/v1\/user\/(sessions|preferences)(?:\?|$)/, async (route) => {
        const path = new URL(route.request().url()).pathname.replace('/api/v1', '');
        api.requests.push({ method: 'GET', path, body: null, query: {} });
        const body = path.endsWith('/sessions') ? { items: [] } : { locale: 'en', available_locales: [{ tag: 'en', name: 'English' }] };
        return route.fulfill({ contentType: 'application/json', body: JSON.stringify(body) });
    });
}

export const test = base.extend<{
    desktopOptions: DesktopOptions;
    desktop: Awaited<ReturnType<typeof installDesktopBridge>>;
}>({
    desktopOptions: [{}, { option: true }],
    desktop: async ({ page, api, desktopOptions }, use) => {
        await installDesktopApi(page, api);
        const desktop = await installDesktopBridge(page, api.user, desktopOptions);
        await use(desktop);
        expect(desktop.unhandled, 'Every mocked native command must match the existing desktop bridge').toEqual([]);
    },
});

export { expect };
