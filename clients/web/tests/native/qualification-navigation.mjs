import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';

export async function exerciseNativeNavigation(page, screenshot) {
    const initialOrigin = new URL(page.url()).origin;
    await invokeNative(page, 'plugin:event|emit', { event: 'duskcue://navigate', payload: { route: '/collections', source: 'qualification' } });
    await expect.poll(() => new URL(page.url()).pathname).toBe('/collections');
    await expect(page.getByRole('heading', { name: 'Collections', exact: true })).toBeVisible();
    await screenshot('native-collections');
    await invokeNative(page, 'plugin:event|emit', { event: 'duskcue://navigate', payload: { route: 'https://example.invalid/outside', source: 'qualification' } });
    await expect(page.getByRole('heading', { name: 'Collections', exact: true })).toBeVisible();
    expect(new URL(page.url()).origin).toBe(initialOrigin);
    expect(new URL(page.url()).pathname).toBe('/collections');
    await invokeNative(page, 'plugin:event|emit', { event: 'duskcue://navigate', payload: { route: '/settings/preferences', source: 'qualification' } });
    await expect.poll(() => new URL(page.url()).pathname).toBe('/settings/preferences');
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).not.toBeChecked();
    return { actualNativeEventDelivery: true, allowedInternalNavigation: true, externalPayloadRejected: true };
}
