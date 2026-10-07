import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';

const path = '/profiles/current/viewing-preferences';

test('saves profile defaults and device quality explicitly and restores them after reload', async ({ page, api }) => {
    await page.goto('/settings/preferences');
    await expect(page.getByRole('group', { name: 'This profile', exact: true })).toBeVisible();
    await page.getByLabel('Play the next episode automatically', { exact: true }).uncheck();
    await page.getByLabel('Preferred audio language', { exact: true }).selectOption('fr');
    await page.getByLabel('Prefer audio description when available', { exact: true }).check();
    await page.getByLabel('Subtitles', { exact: true }).selectOption('always');
    await page.getByLabel('Preferred subtitle language', { exact: true }).selectOption('en');
    await page.getByLabel('Prefer subtitles for deaf and hard-of-hearing viewers', { exact: true }).check();
    await page.getByLabel('Streaming quality', { exact: true }).selectOption('manual');
    await page.getByLabel('Bitrate limit (Mbps)', { exact: true }).selectOption('6000000');
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();

    await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
    expect(api.requests.find((request) => request.path === path && request.method === 'PATCH')).toMatchObject({
        body: { expected_profile_id: profileIds.alex, viewing_preferences: {
            autoplay_next_episode: false, audio_language: 'fr', prefer_audio_description: true,
            subtitle_mode: 'always', subtitle_language: 'en', prefer_sdh: true,
        } },
    });
    await page.reload();
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).not.toBeChecked();
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('fr');
    await expect(page.getByLabel('Preferred subtitle language', { exact: true })).toHaveValue('en');
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('manual');
    await expect(page.getByLabel('Bitrate limit (Mbps)', { exact: true })).toHaveValue('6000000');
    await expect(page.getByRole('button', { name: 'Save changes', exact: true })).toBeDisabled();
});

test('distinguishes explicitly saved defaults from an unsaved profile', async ({ page, api }) => {
    await page.goto('/settings/preferences');
    await expect(page.getByRole('button', { name: 'Save changes', exact: true })).toBeEnabled();
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
    expect(api.preferencesByProfile[profileIds.alex].has_saved_preferences).toBe(true);
    await expect(page.getByRole('button', { name: 'Save changes', exact: true })).toBeDisabled();
});

test('retains a failed save draft and discards back to confirmed defaults', async ({ page, api }) => {
    api.failures.set(`PATCH ${path}`, { status: 503, detail: 'Fixture preference save unavailable' });
    await page.goto('/settings/preferences');
    await page.getByLabel('Preferred audio language', { exact: true }).selectOption('fr');
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await expect(page.getByRole('alert')).toHaveText('Fixture preference save unavailable');
    await expect(page.getByRole('alert')).toBeFocused();
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('fr');
    expect(api.preferencesByProfile[profileIds.alex].viewing_preferences.audio_language).toBeNull();
    await page.getByRole('button', { name: 'Discard changes', exact: true }).click();
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('');
    await expect(page.getByRole('alert')).toHaveCount(0);
    await expect(page.getByRole('main').getByRole('status')).toContainText('Changes discarded.');
});

test('native discard dialog defaults to Keep editing, supports Escape, and guards search and links', async ({ page, api }) => {
    await page.goto('/settings/preferences');
    await page.getByLabel('Preferred audio language', { exact: true }).selectOption('fr');
    await page.getByRole('searchbox', { name: 'Search', exact: true }).fill('moon');
    await page.getByRole('searchbox', { name: 'Search', exact: true }).press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Discard unsaved changes?', exact: true });
    await expect(dialog).toBeVisible();
    await expect(dialog.getByRole('button', { name: 'Keep editing', exact: true })).toBeFocused();
    await page.keyboard.press('Escape');
    await expect(dialog).not.toBeVisible();
    await expect(page).toHaveURL(/\/settings\/preferences$/);
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('fr');
    expect(api.requests.some((request) => request.path === path && request.method === 'PATCH')).toBe(false);

    await page.getByRole('link', { name: 'Duskcue', exact: true }).click();
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Keep editing', exact: true }).click();
    await expect(page).toHaveURL(/\/settings\/preferences$/);
    await page.getByRole('link', { name: 'Duskcue', exact: true }).click();
    await dialog.getByRole('button', { name: 'Discard and continue', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard$/);
});

test('Back navigation preserves history after cancellation and replays it after approved discard', async ({ page, api }) => {
    await page.goto('/dashboard');
    await expect(page.getByRole('heading', { name: 'Your evening starts here.', exact: true })).toBeVisible();
    expect(api.activeProfileId).toBe(profileIds.alex);
    await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
    await page.getByRole('link', { name: 'Viewing preferences', exact: true }).click();
    await page.getByLabel('Preferred audio language', { exact: true }).selectOption('fr');
    await page.goBack();
    const dialog = page.getByRole('dialog', { name: 'Discard unsaved changes?', exact: true });
    await expect(dialog).toBeVisible();
    const closed = dialog.evaluate((element) => new Promise<void>((resolve) => {
        element.addEventListener('close', () => resolve(), { once: true });
    }));
    await dialog.getByRole('button', { name: 'Keep editing', exact: true }).click();
    await closed;
    await expect(page).toHaveURL(/\/settings\/preferences$/);
    await page.goBack();
    await expect(dialog).toBeVisible();
    await dialog.getByRole('button', { name: 'Discard and continue', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard$/);
    await page.goForward();
    await expect(page).toHaveURL(/\/settings\/preferences$/);
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('');
});

test('profile switching requires discard approval and loads isolated defaults while sharing device quality', async ({ page, api }) => {
    api.preferencesByProfile[profileIds.alex] = {
        profile_id: profileIds.alex, has_saved_preferences: true,
        viewing_preferences: { autoplay_next_episode: false, audio_language: 'fr', prefer_audio_description: false, subtitle_mode: 'none', subtitle_language: null, prefer_sdh: false },
    };
    await page.goto('/settings/preferences');
    await page.getByLabel('Streaming quality', { exact: true }).selectOption('maximum');
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
    await page.getByLabel('Preferred audio language', { exact: true }).selectOption('de');
    await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
    await page.getByRole('button', { name: /Morgan$/ }).click();
    const dialog = page.getByRole('dialog', { name: 'Discard unsaved changes?', exact: true });
    await expect(dialog).toBeVisible();
    expect(api.requests.some((request) => request.path === `/profiles/${profileIds.morgan}/switch`)).toBe(false);
    await dialog.getByRole('button', { name: 'Keep editing', exact: true }).click();
    expect(api.activeProfileId).toBe(profileIds.alex);
    await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
    await page.getByRole('button', { name: /Morgan$/ }).click();
    await dialog.getByRole('button', { name: 'Discard and continue', exact: true }).click();
    await expect(page).toHaveURL(/\/dashboard$/);
    await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
    await page.getByRole('link', { name: 'Viewing preferences', exact: true }).click();
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('');
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).toBeChecked();
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('maximum');
    expect(api.activeProfileId).toBe(profileIds.morgan);
    expect(api.preferencesByProfile[profileIds.alex].viewing_preferences.audio_language).toBe('fr');
});

test('a local device write failure reports partial success and retains the unsaved quality draft', async ({ page, api }) => {
    await page.addInitScript(() => {
        const setItem = Storage.prototype.setItem;
        Storage.prototype.setItem = function (key, value) {
            if (key.startsWith('duskcue_viewing_device_prefs_v1:')) throw new Error('Fixture storage unavailable');
            setItem.call(this, key, value);
        };
    });
    await page.goto('/settings/preferences');
    await page.getByLabel('Play the next episode automatically', { exact: true }).uncheck();
    await page.getByLabel('Streaming quality', { exact: true }).selectOption('manual');
    await page.getByRole('button', { name: 'Save changes', exact: true }).click();
    await expect(page.getByRole('alert')).toContainText('Profile defaults were saved');
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('manual');
    await expect(page.getByRole('button', { name: 'Discard changes', exact: true })).toBeEnabled();
    expect(api.preferencesByProfile[profileIds.alex].has_saved_preferences).toBe(true);
    expect(api.preferencesByProfile[profileIds.alex].viewing_preferences.autoplay_next_episode).toBe(false);
    await page.getByRole('button', { name: 'Discard changes', exact: true }).click();
    await expect(page.getByLabel('Streaming quality', { exact: true })).toHaveValue('auto');
    await expect(page.getByLabel('Play the next episode automatically', { exact: true })).not.toBeChecked();
});

test.describe('required selection and Kids self-service', () => {
    test.use({ scenarioOptions: { selectionRequired: true } });

    test('withholds preference requests until selection and permits Kids playback defaults', async ({ page, api }) => {
        await page.goto('/settings/preferences');
        await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
        expect(api.requests.some((request) => request.path === path)).toBe(false);
        await page.getByRole('button', { name: /(?:Kids|Casey)/ }).click();
        await expect(page).toHaveURL(/\/dashboard$/);
        await page.getByRole('button', { name: /(?:User|Profile) menu/ }).click();
        await page.getByRole('link', { name: 'Viewing preferences', exact: true }).click();
        await expect(page.getByRole('heading', { name: 'Viewing preferences', exact: true })).toBeVisible();
        await page.getByLabel('Play the next episode automatically', { exact: true }).uncheck();
        await page.getByRole('button', { name: 'Save changes', exact: true }).click();
        await expect(page.getByRole('main').getByRole('status')).toContainText('Changes saved.');
        expect(api.preferencesByProfile[profileIds.kids].viewing_preferences.autoplay_next_episode).toBe(false);
        expect(api.requests.some((request) => request.path === '/profiles/parent-unlock')).toBe(false);
    });
});
