import { test, expect } from './fixtures';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

test('real dashboard route loads profile-scoped media and navigates to its title', async ({ page, api }) => {
    await page.goto('/dashboard');

    await expect(page.getByRole('heading', { name: 'Your evening starts here.' })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'Recently added', exact: true })).toBeVisible();
    const titleLink = page.getByRole('link', { name: /^The Lighthouse Archive/ }).and(page.locator('a[href*="/media/"]')).first();
    await expect(titleLink).toBeVisible();
    await titleLink.click();

    await expect(page).toHaveURL(new RegExp(`/media/${mediaIds.resumeMovie}(?:\\?|$)`));
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Resume', exact: true })).toBeEnabled();
    expect(api.requests.some((request) => request.path === `/items/${mediaIds.resumeMovie}/watch-data`)).toBe(true);
});

test('failed favorite save rolls back without losing the title', async ({ page, api }) => {
    api.failures.set(`PUT /items/${mediaIds.resumeMovie}/watch-data`, { status: 503, detail: 'Fixture favorite save unavailable' });
    await page.goto(`/media/${mediaIds.resumeMovie}`);
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeVisible();

    await page.getByRole('button', { name: 'Favorite', exact: true }).click();

    await expect(page.getByRole('button', { name: 'Favorite', exact: true })).toHaveAttribute('aria-pressed', 'false');
    await expect(page.getByRole('alert')).toContainText('Fixture favorite save unavailable');
    expect(api.watchByProfile[profileIds.alex][mediaIds.resumeMovie].is_favorite).toBe(false);
});

test('unavailable files disable the real title playback action', async ({ page, api }) => {
    await page.goto(`/media/${mediaIds.unavailableMovie}`);

    await expect(page.getByRole('heading', { name: 'Unavailable Feature', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeDisabled();
    expect(api.requests.some((request) => request.path.endsWith('/files'))).toBe(true);
});

test.describe('required profile selection', () => {
    test.use({ scenarioOptions: { selectionRequired: true } });

    test('blocks catalog requests until the picker authorizes a profile', async ({ page, api }) => {
        await page.goto('/dashboard');
        await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
        expect(api.requests.some((request) => request.path === '/media-items')).toBe(false);
        await page.getByRole('button', { name: /Morgan$/ }).click();

        await expect(page.getByRole('heading', { name: 'Your evening starts here.' })).toBeVisible();
        await expect(page.getByRole('heading', { name: 'Recently added', exact: true })).toBeVisible();
        expect(api.activeProfileId).toBe(profileIds.morgan);
        expect(api.requests.find((request) => request.path === `/profiles/${profileIds.morgan}/switch`)).toMatchObject({
            method: 'POST',
            body: { remember_on_device: false },
        });
    });
});

test.describe('profile loading failure', () => {
    test.use({ scenarioOptions: { selectionRequired: true } });

    test('keeps browsing gated and recovers through the actual retry action', async ({ page, api }) => {
        api.failures.set('GET /profiles', { status: 503, detail: 'Fixture profiles unavailable' });
        await page.goto('/dashboard');
        await expect(page.getByRole('heading', { name: 'Profiles are unavailable', exact: true })).toBeVisible();
        expect(api.requests.some((request) => request.path === '/media-items')).toBe(false);

        api.failures.delete('GET /profiles');
        await page.getByRole('button', { name: 'Try again', exact: true }).click();

        await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
    });
});

test.describe('protected Kids exit', () => {
    test.use({ scenarioOptions: { activeProfileId: profileIds.kids } });

    test('requires successful parent unlock before a standard-profile switch', async ({ page, api }) => {
        await page.goto('/dashboard');
        await expect(page.getByRole('link', { name: /^Cloud Parade/ }).first()).toBeVisible();
        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toHaveCount(0);
        await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
        await page.getByRole('button', { name: /Alex$/ }).click();
        await expect(page.getByRole('dialog', { name: 'Enter your parent PIN' })).toBeVisible();
        expect(api.requests.some((request) => request.path === `/profiles/${profileIds.alex}/switch`)).toBe(false);

        await page.getByLabel('Parent PIN', { exact: true }).fill('0000');
        await page.getByRole('button', { name: 'Unlock', exact: true }).click();
        await expect(page.getByText('Fixture parent PIN rejected', { exact: true })).toBeVisible();
        expect(api.activeProfileId).toBe(profileIds.kids);

        await page.getByLabel('Parent PIN', { exact: true }).fill('1357');
        await page.getByRole('button', { name: 'Unlock', exact: true }).click();
        await expect(page.getByRole('dialog', { name: 'Enter your parent PIN' })).toHaveCount(0);
        await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
        expect(api.activeProfileId).toBe(profileIds.alex);
    });
});

test.describe('anonymous session', () => {
    test.use({ scenarioOptions: { authenticated: false } });

    test('redirects a protected production route to sign in', async ({ page, api }) => {
        await page.goto('/dashboard');

        await expect(page).toHaveURL(/\/auth\/login$/);
        await expect(page.getByRole('heading', { name: 'Sign In', exact: true })).toBeVisible();
        expect(api.requests.some((request) => request.path === '/media-items')).toBe(false);
    });
});
