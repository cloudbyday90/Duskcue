import { test as base, expect } from './playback-fixtures';
import { installDesktopApi } from './desktop-fixtures';
import { installDesktopBridge, type DesktopOptions } from '../fixtures/desktop-bridge';

export const test = base.extend<{
    desktopOptions: DesktopOptions;
    desktop: Awaited<ReturnType<typeof installDesktopBridge>>;
}>({
    desktopOptions: [{}, { option: true }],
    desktop: async ({ page, api, desktopOptions }, use) => {
        await installDesktopApi(page, api);
        const desktop = await installDesktopBridge(page, api.user, desktopOptions);
        await use(desktop);
        expect(desktop.unhandled, 'Every mocked native command must match the installed Tauri API').toEqual([]);
    },
});

export { expect };
