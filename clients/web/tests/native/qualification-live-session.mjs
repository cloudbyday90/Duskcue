// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { expect } from '@playwright/test';
import { invokeNative } from './qualification-api.mjs';

export async function connectNativeLive(page, scenario) {
    await page.getByLabel('Server URL', { exact: true }).fill(scenario.origin);
    await page.getByLabel('Network mode', { exact: true }).selectOption('local');
    await page.getByRole('button', { name: 'Test and connect', exact: true }).click();
    await expect.poll(() => new URL(page.url()).pathname).toBe('/auth/login');
    const saved = await invokeNative(page, 'read_server_connections');
    if (saved.last_server?.origin !== scenario.origin) throw new Error('Native server selection was not persisted.');
    await invokeNative(page, 'write_session_token', { req: { server_origin: scenario.origin, token: scenario.token } });
    if (await invokeNative(page, 'read_session_token', { req: { server_origin: scenario.origin } }) !== scenario.token) throw new Error('Native keyring round trip failed.');
    await page.evaluate(({ key, user }) => localStorage.setItem(key, JSON.stringify(user)), { key: scenario.userStorageKey, user: scenario.user });
    const applicationOrigin = new URL(page.url()).origin;
    await page.goto(`${applicationOrigin}/play/${scenario.candidate.item}?file=${scenario.candidate.file}`);
    return { nativeHealthAndServerSelection: true, actualKeyringRoundTrip: true, restoredRealBearerSession: true };
}

export async function clearNativeLiveCredential(page, scenario) {
    const current = await invokeNative(page, 'read_session_token', { req: { server_origin: scenario.origin } });
    if (current !== null && current !== scenario.token) throw new Error('Unexpected native credential was preserved.');
    if (current !== null) await invokeNative(page, 'clear_session_token', { req: { server_origin: scenario.origin } });
    if (await invokeNative(page, 'read_session_token', { req: { server_origin: scenario.origin } }) !== null) throw new Error('Native fixture credential was not cleared.');
    await page.evaluate((key) => localStorage.removeItem(key), scenario.userStorageKey);
    return { exactOwnedCredentialCleared: true };
}
