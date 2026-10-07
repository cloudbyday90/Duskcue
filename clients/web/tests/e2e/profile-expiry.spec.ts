import { test, expect } from './fixtures';
import { profileIds } from '../fixtures/catalog.js';

const expiryMessage = 'Fixture parent access expired. Enter your PIN again.';
const expiryCopy = 'Parent access expired. Enter your PIN again to switch profiles.';

test.use({ scenarioOptions: { activeProfileId: profileIds.kids } });

test('an expired parent grant keeps Kids active and requests a fresh PIN before retrying the standard switch', async ({ page, api }) => {
    let grants = 0;
    let switchAttempts = 0;
    let denied = false;
    let documentLoads = 0;
    page.on('domcontentloaded', () => { documentLoads += 1; });
    await page.route(/\/api\/v1\/profiles\/(?:parent-unlock|[^/]+\/switch)(?:\?|$)/, async (route) => {
        const request = route.request();
        const url = new URL(request.url());
        const path = url.pathname.replace('/api/v1', '');
        const body = request.postData() ? request.postDataJSON() : null;
        if (request.method() === 'POST' && path === '/profiles/parent-unlock' && body?.pin === '1357') {
            grants += 1;
        }
        if (request.method() === 'POST' && path === `/profiles/${profileIds.alex}/switch`) {
            switchAttempts += 1;
            if (grants === 1 && !denied) {
                denied = true;
                api.parentUnlockRequired = true;
                api.requests.push({ method: 'POST', path, body, query: Object.fromEntries(url.searchParams) });
                return route.fulfill({ status: 403, contentType: 'application/problem+json', body: JSON.stringify({
                    type: '/errors/PROFILE_012', title: 'PROFILE_012', status: 403, detail: expiryMessage, trace_id: 'fixture-expired-parent-grant',
                }) });
            }
        }
        return route.fallback();
    });

    await page.goto('/dashboard');
    await expect(page.getByRole('link', { name: /^Cloud Parade/ }).first()).toBeVisible();
    await expect(page.getByRole('link', { name: /^Completed Journey/ })).toHaveCount(0);
    await page.getByRole('button', { name: 'User menu', exact: true }).click();
    await page.getByRole('button', { name: /Alex$/ }).click();
    const dialog = page.getByRole('dialog', { name: 'Enter your parent PIN', exact: true });
    const pin = dialog.getByLabel('Parent PIN', { exact: true });
    await expect(dialog).toBeVisible();
    await expect(pin).toBeFocused();
    expect(switchAttempts).toBe(0);
    await pin.fill('1357');
    await dialog.getByRole('button', { name: 'Unlock', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect.poll(() => denied).toBe(true);
    await expect(dialog).toBeVisible();
    await expect(pin).toHaveValue('');
    await expect(pin).toBeFocused();
    await expect(dialog.getByRole('status')).toHaveText(expiryCopy);
    expect(grants).toBe(1);
    expect(switchAttempts).toBe(1);
    expect(api.activeProfileId).toBe(profileIds.kids);
    await expect(page.getByRole('link', { name: /^Cloud Parade/, includeHidden: true }).first()).toBeVisible();
    await expect(page.getByRole('link', { name: /^Completed Journey/, includeHidden: true })).toHaveCount(0);
    expect(documentLoads).toBe(1);

    await pin.fill('1357');
    await dialog.getByRole('button', { name: 'Unlock', exact: true }).focus();
    await page.keyboard.press('Enter');
    await expect(dialog).toHaveCount(0);
    await expect.poll(() => api.activeProfileId).toBe(profileIds.alex);
    await expect(page.getByRole('link', { name: /^Completed Journey/ })).toBeVisible();
    expect(grants).toBe(2);
    expect(switchAttempts).toBe(2);
    expect(documentLoads).toBe(1);
});
