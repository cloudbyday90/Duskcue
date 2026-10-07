import type { Locator, Page } from '@playwright/test';
import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';
import type { createApiScenario } from '../fixtures/api.js';

type Scenario = ReturnType<typeof createApiScenario>;

function profileCard(page: Page, name: string) {
    return page.getByRole('main').locator('section').filter({ has: page.getByRole('heading', { name, exact: true }) });
}

function nameField(card: Locator) {
    return card.getByRole('textbox', { name: /^Profile name/ });
}

async function installNameUpdates(page: Page, api: Scenario) {
    const patches: { profileId: string; body: Record<string, unknown> }[] = [];
    await page.route(/\/api\/v1(?:\/|$)/, async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const match = url.pathname.match(/^\/api\/v1\/profiles\/([^/]+)$/);
        if (request.method() !== 'PATCH' || !match) return route.fallback();
        const profileId = match[1];
        const body = request.postDataJSON();
        const path = `/profiles/${profileId}`;
        patches.push({ profileId, body });
        api.requests.push({ method: 'PATCH', path, body, query: Object.fromEntries(url.searchParams) });
        const failure = api.failures.get(`PATCH ${path}`);
        if (failure) return route.fulfill({
            status: failure.status, contentType: 'application/problem+json',
            body: JSON.stringify({ type: '/errors/fixture', title: 'PROFILE_NAME_UNAVAILABLE', status: failure.status, detail: failure.detail }),
        });
        const original = api.profiles.find((profile) => profile.id === profileId);
        if (!original) return route.fulfill({ status: 404, contentType: 'application/problem+json', body: JSON.stringify({ status: 404, detail: 'Fixture profile not found' }) });
        const updated = { ...original, name: body.name, updated_at: '2026-10-03T13:00:00Z' };
        api.profiles = api.profiles.map((profile) => profile.id === profileId ? updated : profile);
        return route.fulfill({ contentType: 'application/json', body: JSON.stringify(updated) });
    });
    return patches;
}

test('account-owned standard profile submits only the trimmed name with the native keyboard form', async ({ page, api }) => {
    const patches = await installNameUpdates(page, api);
    await page.goto('/settings/profiles');
    const card = profileCard(page, 'Alex');
    const input = nameField(card);
    await expect(input).toHaveValue('Alex');
    await expect(card.getByRole('form', { name: 'Edit profile name for Alex', exact: true })).toBeVisible();
    await expect(input).toHaveAttribute('required', '');
    await expect(input).toHaveAttribute('maxlength', '80');
    await input.fill('  Alex Evening  ');
    expect(patches).toEqual([]);
    await input.press('Enter');
    const updatedCard = profileCard(page, 'Alex Evening');
    await expect(updatedCard.getByRole('status')).toHaveText('Profile name saved.');
    await expect(nameField(updatedCard)).toBeFocused();
    expect(patches).toEqual([{ profileId: profileIds.alex, body: { name: 'Alex Evening' } }]);
    expect(api.profiles.find((profile) => profile.id === profileIds.alex)).toMatchObject({ profile_type: 'standard', is_default: true });
    await expect(updatedCard.getByRole('button', { name: 'Save name', exact: true })).toBeDisabled();
});

test('Kids profile name save preserves all parental policy and uses no PIN or policy payload', async ({ page, api }) => {
    const original = structuredClone(api.profiles.find((profile) => profile.id === profileIds.kids)!);
    const patches = await installNameUpdates(page, api);
    await page.goto('/settings/profiles');
    const card = profileCard(page, 'Kids');
    await nameField(card).fill('Little Viewers');
    const save = card.getByRole('button', { name: 'Save name', exact: true });
    await save.focus();
    await save.press('Enter');
    const updatedCard = profileCard(page, 'Little Viewers');
    await expect(updatedCard.getByRole('status')).toHaveText('Profile name saved.');
    await expect(nameField(updatedCard)).toBeFocused();
    expect(patches).toEqual([{ profileId: profileIds.kids, body: { name: 'Little Viewers' } }]);
    expect(api.profiles.find((profile) => profile.id === profileIds.kids)).toEqual({ ...original, name: 'Little Viewers', updated_at: '2026-10-03T13:00:00Z' });
    await expect(updatedCard.getByRole('combobox', { name: 'Maximum rating', exact: true })).toHaveValue(original.max_content_rating);
    await expect(updatedCard.getByRole('button', { name: 'Save controls', exact: true })).toBeVisible();
    expect(api.requests.some((request) => request.path === '/profiles/parent-unlock')).toBe(false);
});

test('saved active profile name refreshes the navbar and survives a complete page reload', async ({ page, api }) => {
    const patches = await installNameUpdates(page, api);
    await page.goto('/settings/profiles');
    const menu = page.getByRole('button', { name: /(?:User|Profile) menu/ });
    await expect(menu).toContainText('Alex');
    const readsBeforeSave = api.requests.filter((request) => request.method === 'GET' && request.path === '/profiles').length;
    const card = profileCard(page, 'Alex');
    await nameField(card).fill('Alex Tonight');
    await card.getByRole('button', { name: 'Save name', exact: true }).click();
    await expect(menu).toContainText('Alex Tonight');
    expect(api.requests.filter((request) => request.method === 'GET' && request.path === '/profiles').length).toBeGreaterThan(readsBeforeSave);
    expect(patches).toEqual([{ profileId: profileIds.alex, body: { name: 'Alex Tonight' } }]);
    await page.reload();
    await expect(nameField(profileCard(page, 'Alex Tonight'))).toHaveValue('Alex Tonight');
    await expect(menu).toContainText('Alex Tonight');
    await menu.click();
    await expect(page.getByRole('button', { name: 'Alex Tonight', exact: true })).toHaveAttribute('aria-pressed', 'true');
});

test('a failed name PATCH retains the draft and supports an explicit keyboard Save-name retry', async ({ page, api }) => {
    const failureKey = `PATCH /profiles/${profileIds.morgan}`;
    api.failures.set(failureKey, { status: 503, detail: 'Fixture profile name save unavailable' });
    const patches = await installNameUpdates(page, api);
    await page.goto('/settings/profiles');
    const card = profileCard(page, 'Morgan');
    const input = nameField(card);
    await input.fill('Morgan Cinema');
    await input.press('Enter');
    await expect(card.getByRole('alert')).toHaveText('Fixture profile name save unavailable');
    await expect(input).toHaveValue('Morgan Cinema');
    await expect(input).toBeFocused();
    await expect(card.getByRole('button', { name: 'Save name', exact: true })).toBeEnabled();
    expect(api.profiles.find((profile) => profile.id === profileIds.morgan)!.name).toBe('Morgan');
    api.failures.delete(failureKey);
    await input.press('Enter');
    const updatedCard = profileCard(page, 'Morgan Cinema');
    await expect(updatedCard.getByRole('status')).toHaveText('Profile name saved.');
    await expect(updatedCard.getByRole('alert')).toHaveCount(0);
    await expect(nameField(updatedCard)).toBeFocused();
    expect(patches).toEqual([
        { profileId: profileIds.morgan, body: { name: 'Morgan Cinema' } },
        { profileId: profileIds.morgan, body: { name: 'Morgan Cinema' } },
    ]);
});

test('keyboard Discard restores the confirmed profile name and focus without a mutation', async ({ page, api }) => {
    const patches = await installNameUpdates(page, api);
    await page.goto('/settings/profiles');
    const card = profileCard(page, 'Morgan');
    const input = nameField(card);
    await input.fill('   ');
    await expect(card.getByRole('button', { name: 'Save name', exact: true })).toBeDisabled();
    await input.fill('Unsaved name');
    const discard = card.getByRole('button', { name: 'Discard changes', exact: true });
    await discard.focus();
    await discard.press('Enter');
    await expect(input).toHaveValue('Morgan');
    await expect(input).toBeFocused();
    await expect(discard).toHaveCount(0);
    await expect(card.getByRole('button', { name: 'Save name', exact: true })).toBeDisabled();
    await expect(card.getByRole('alert')).toHaveCount(0);
    expect(patches).toEqual([]);
});
