import { expect } from '@playwright/test';
import { keyboardReach } from './qualification-focus.mjs';

export async function exerciseProfileZoom({ page, api, evidence, record, capture }) {
    const menu = page.getByRole('button', { name: 'User menu', exact: true });
    await keyboardReach(page, menu, 'profile-menu-trigger', evidence);
    await page.keyboard.press('Enter');
    await expect(menu).toHaveAttribute('aria-expanded', 'true');
    for (const [name, label] of [['Alex', 'profile-alex'], ['Morgan', 'profile-morgan']]) {
        await keyboardReach(page, page.getByRole('button', { name, exact: true }), label, evidence);
    }
    await keyboardReach(page, page.getByRole('button', { name: /^Kids/ }), 'profile-kids', evidence);
    await record('profile-menu');
    await page.keyboard.press('Escape');
    await expect(menu).toHaveAttribute('aria-expanded', 'false');
    await expect(menu).toBeFocused();

    api.scenario.selectionRequired = true;
    api.scenario.user.profile_selection_required = true;
    await page.reload();
    await expect(page.getByRole('heading', { name: 'Who’s watching?', exact: true })).toBeVisible();
    for (const [name, label] of [['Alex', 'picker-alex'], ['Morgan', 'picker-morgan']]) {
        await keyboardReach(page, page.getByRole('button', { name, exact: true }), label, evidence);
    }
    await keyboardReach(page, page.getByRole('button', { name: /^Kids/ }), 'picker-kids', evidence);
    await record('profile-picker');
    await keyboardReach(page, page.getByRole('button', { name: 'Alex', exact: true }), 'picker-select-alex', evidence);
    await page.keyboard.press('Enter');
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    expect(api.scenario.selectionRequired).toBe(false);
    api.scenario.user.profile_selection_required = false;
    await capture('native-400-profile-selected');
    return { disclosureChoicesExposed: true, escapeReturnedTrigger: true, requiredPickerExposed: true, actualProfileSwitchCommand: true };
}

export async function exerciseDiscardZoom({ page, evidence, record, capture }) {
    const audio = page.getByLabel('Preferred audio language', { exact: true });
    await audio.selectOption('fr');
    const home = page.getByRole('link', { name: 'Home', exact: true }).first();
    await keyboardReach(page, home, 'preferences-unsaved-home', evidence);
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Discard unsaved changes?', exact: true });
    await expect(dialog).toBeVisible();
    await keyboardReach(page, dialog.getByRole('button', { name: 'Keep editing', exact: true }), 'discard-keep-editing', evidence);
    await keyboardReach(page, dialog.getByRole('button', { name: 'Discard changes', exact: true }), 'discard-confirm-action', evidence);
    await record('discard-dialog');
    await page.keyboard.press('Escape');
    await expect(dialog).not.toBeVisible();
    await expect(home).toBeFocused();
    await expect(audio).toHaveValue('fr');
    await keyboardReach(page, page.getByRole('button', { name: 'Discard changes', exact: true }), 'preferences-discard-draft', evidence);
    await page.keyboard.press('Enter');
    await expect(audio).toHaveValue('');
    await capture('native-400-draft-discarded');
    return { modalActionsExposed: true, escapeKeptDraftAndReturnedInvoker: true, explicitDiscardRestoredConfirmedPreferences: true };
}
