import { test, expect } from './fixtures';
import { keyboardReach, noHorizontalOverflow, textContrast } from './accessibility-journey';
import { mediaIds, profileIds } from '../fixtures/catalog.js';

test.use({ viewport: { width: 320, height: 180 }, scenarioOptions: { browsePageSize: 1 } });

test('Home, preference fields and discard actions stay exposed through ordinary keyboard scrolling in a short viewport', async ({ page, api }) => {
    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.goto('/dashboard');
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    const search = page.getByRole('searchbox', { name: 'Search', exact: true });
    await keyboardReach(page, search);
    expect((await textContrast(search)).ratio).toBeGreaterThanOrEqual(4.5);
    await noHorizontalOverflow(page);
    await page.screenshot({ path: test.info().outputPath('home-short-focus.png') });

    await page.goto('/settings/preferences');
    const language = page.getByLabel('Preferred audio language', { exact: true });
    await keyboardReach(page, language);
    expect((await textContrast(language)).ratio).toBeGreaterThanOrEqual(4.5);
    await language.selectOption('fr');
    await noHorizontalOverflow(page);
    await page.screenshot({ path: test.info().outputPath('preferences-short-focus.png') });
    await keyboardReach(page, page.getByRole('link', { name: 'Interface language', exact: true }));
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Discard unsaved changes?', exact: true });
    await expect(dialog).toBeVisible();
    const keep = dialog.getByRole('button', { name: 'Keep editing', exact: true });
    await keyboardReach(page, keep);
    expect((await textContrast(keep)).ratio).toBeGreaterThanOrEqual(4.5);
    await keyboardReach(page, dialog.getByRole('button', { name: 'Discard and continue', exact: true }));
    await page.screenshot({ path: test.info().outputPath('discard-short-focus.png') });
    await page.keyboard.press('Escape');
    await expect(dialog).not.toBeVisible();
    await expect(page).toHaveURL(/\/settings\/preferences$/);
    await expect(page.getByLabel('Preferred audio language', { exact: true })).toHaveValue('fr');
});

test('Title actions and every profile disclosure choice remain visible when keyboard focused', async ({ page, api }) => {
    await page.goto(`/media/${mediaIds.resumeMovie}`);
    const resume = page.getByRole('button', { name: 'Resume', exact: true });
    await expect(resume).toBeEnabled();
    await keyboardReach(page, resume);
    expect((await textContrast(resume)).ratio).toBeGreaterThanOrEqual(4.5);
    await noHorizontalOverflow(page);
    await page.screenshot({ path: test.info().outputPath('title-short-focus.png') });
    await page.goto('/dashboard');
    const trigger = page.getByRole('button', { name: 'User menu', exact: true });
    await keyboardReach(page, trigger);
    await page.keyboard.press('Enter');
    const panel = page.locator('#profile-disclosure');
    await expect(panel).toBeVisible();
    for (const control of await panel.locator('button:not([disabled]), a, input:not([disabled])').all()) await keyboardReach(page, control);
    await page.screenshot({ path: test.info().outputPath('profile-disclosure-short-focus.png') });
    await page.keyboard.press('Escape');
    await expect(panel).toHaveCount(0);
    await keyboardReach(page, trigger);
    await noHorizontalOverflow(page);
});

test.describe('short-height protected Kids exit', () => {
    test.use({ scenarioOptions: { activeProfileId: profileIds.kids } });
    test('parent PIN and cancellation scroll within the native modal and return focus', async ({ page, api }) => {
        await page.goto('/dashboard');
        const trigger = page.getByRole('button', { name: 'User menu', exact: true });
        await keyboardReach(page, trigger);
        await page.keyboard.press('Enter');
        await keyboardReach(page, page.getByRole('button', { name: /Alex$/ }));
        await page.keyboard.press('Enter');
        const dialog = page.getByRole('dialog', { name: 'Enter your parent PIN', exact: true });
        await expect(dialog).toBeVisible();
        const pin = dialog.getByLabel('Parent PIN', { exact: true });
        await keyboardReach(page, pin);
        expect((await textContrast(pin)).ratio).toBeGreaterThanOrEqual(4.5);
        await pin.fill('0000');
        await keyboardReach(page, dialog.getByRole('button', { name: 'Unlock', exact: true }));
        await page.keyboard.press('Enter');
        await expect(dialog.getByRole('status')).toContainText('Fixture parent PIN rejected');
        await keyboardReach(page, dialog.getByRole('button', { name: 'Cancel', exact: true }));
        await page.screenshot({ path: test.info().outputPath('parent-pin-short-focus.png') });
        await page.keyboard.press('Enter');
        await expect(dialog).toHaveCount(0);
        expect(api.activeProfileId).toBe(profileIds.kids);
        await keyboardReach(page, trigger);
        await noHorizontalOverflow(page);
    });
});
