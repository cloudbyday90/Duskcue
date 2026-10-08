// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';

test.use({ scenarioOptions: { selectionRequired: true } });

test('explicit remembering pins the chosen profile across ordinary switches until Forget removes it', async ({ page, api }) => {
    const profilesBefore = structuredClone(api.profiles);
    const switches = () => api.requests
        .filter((request) => request.method === 'POST' && /^\/profiles\/[^/]+\/switch$/.test(request.path))
        .map(({ path, body }) => ({ path, body }));
    const menu = page.getByRole('button', { name: 'User menu', exact: true });
    const disclosure = page.locator('#profile-disclosure');
    const rememberLabel = 'Remember this profile on this device';
    const forget = disclosure.getByRole('button', { name: 'Forget this device', exact: true });

    await page.goto('/dashboard');
    await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
    expect(api.requests.some((request) => request.path === '/media-items')).toBe(false);
    await page.getByRole('checkbox', { name: rememberLabel, exact: true }).check();
    await page.getByRole('main').getByRole('button', { name: /Alex$/ }).click();
    await expect(menu).toContainText('Alex');
    await expect(page.getByRole('heading', { name: 'Your evening starts here.', exact: true })).toBeVisible();
    expect(api.selectionRequired).toBe(false);
    expect(api.activeProfileId).toBe(profileIds.alex);
    expect(api.rememberedProfileId).toBe(profileIds.alex);
    expect(switches()).toEqual([{ path: `/profiles/${profileIds.alex}/switch`, body: { remember_on_device: true } }]);

    await menu.click();
    await expect(forget).toBeVisible();
    await disclosure.getByRole('button', { name: 'Morgan', exact: true }).click();
    await expect(menu).toContainText('Morgan');
    await expect(menu).toHaveAttribute('aria-expanded', 'false');
    expect(api.activeProfileId).toBe(profileIds.morgan);
    expect(api.rememberedProfileId).toBe(profileIds.alex);

    await menu.click();
    const remember = disclosure.getByRole('checkbox', { name: rememberLabel, exact: true });
    await expect(remember).not.toBeChecked();
    await expect(forget).toHaveCount(0);
    await remember.check();
    await expect(forget).toBeVisible();
    expect(api.activeProfileId).toBe(profileIds.morgan);
    expect(api.rememberedProfileId).toBe(profileIds.morgan);

    await disclosure.getByRole('button', { name: 'Alex', exact: true }).click();
    await expect(menu).toContainText('Alex');
    await expect(menu).toHaveAttribute('aria-expanded', 'false');
    expect(api.activeProfileId).toBe(profileIds.alex);
    expect(api.rememberedProfileId).toBe(profileIds.morgan);
    await menu.click();
    await expect(remember).not.toBeChecked();
    await disclosure.getByRole('button', { name: 'Morgan', exact: true }).click();
    await expect(menu).toContainText('Morgan');
    await expect(menu).toHaveAttribute('aria-expanded', 'false');
    await menu.click();
    await expect(forget).toBeVisible();
    await forget.click();
    await expect(remember).not.toBeChecked();
    await expect(remember).toBeEnabled();
    await expect(forget).toHaveCount(0);
    expect(api.activeProfileId).toBe(profileIds.morgan);
    expect(api.rememberedProfileId).toBeNull();
    expect(switches()).toEqual([
        { path: `/profiles/${profileIds.alex}/switch`, body: { remember_on_device: true } },
        { path: `/profiles/${profileIds.morgan}/switch`, body: {} },
        { path: `/profiles/${profileIds.morgan}/switch`, body: { remember_on_device: true } },
        { path: `/profiles/${profileIds.alex}/switch`, body: {} },
        { path: `/profiles/${profileIds.morgan}/switch`, body: {} },
        { path: `/profiles/${profileIds.morgan}/switch`, body: { remember_on_device: false } },
    ]);
    expect(api.profiles).toEqual(profilesBefore);
    expect(api.requests.some((request) => request.path === '/profiles/parent-unlock')).toBe(false);
});
