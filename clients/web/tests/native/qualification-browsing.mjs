import { expect } from '@playwright/test';
import { mediaIds } from '../fixtures/catalog.js';

const locationPath = (page) => new URL(page.url()).pathname + new URL(page.url()).search;

async function returnToOrigin(page, origin) {
    const back = page.getByRole('link', { name: /Back to browsing/ });
    await expect(back).toHaveAttribute('href', origin);
    await back.click();
    await expect.poll(() => locationPath(page)).toBe(origin);
}

export async function exerciseNativeBrowsing(page, api, screenshot) {
    await page.getByRole('link', { name: 'Movies', exact: true }).click();
    await expect(page.getByRole('heading', { name: 'Movies', exact: true })).toBeVisible();
    const movie = page.getByRole('link', { name: /^The Lighthouse Archive,/ });
    await expect(movie).toBeVisible();
    await screenshot('native-catalog');
    const catalog = locationPath(page);
    await movie.click();
    await expect(page.getByRole('heading', { name: 'The Lighthouse Archive', exact: true })).toBeVisible();
    await expect(page.getByRole('button', { name: /^(Play|Resume)$/ })).toBeEnabled();
    expect(new URL(page.url()).pathname).toBe(`/media/${mediaIds.resumeMovie}`);
    expect(new URL(page.url()).searchParams.get('from')).toBe(catalog);
    await screenshot('native-movie-title');
    await returnToOrigin(page, catalog);

    const before = api.scenario.requests.length;
    const search = page.getByRole('searchbox', { name: 'Search', exact: true });
    await search.fill('Harbor');
    await search.press('Enter');
    await expect.poll(() => new URL(page.url()).pathname).toBe('/search');
    expect(new URL(page.url()).searchParams.get('q')).toBe('Harbor');
    await expect(page.getByRole('heading', { name: 'Results for “Harbor”', exact: true })).toBeVisible();
    const series = page.locator(`a[href^="/media/${mediaIds.series}"]`).first();
    await expect(series).toBeVisible();
    await expect(series).toContainText('Harbor Stories');
    expect(api.scenario.requests.slice(before).some((request) => request.method === 'GET' && request.path === '/search' && request.query.q === 'Harbor')).toBe(true);
    await screenshot('native-search');
    const submittedSearch = locationPath(page);
    await series.click();
    await expect(page.getByRole('heading', { name: 'Harbor Stories', exact: true })).toBeVisible();
    await expect(page.getByRole('heading', { name: 'The Last Ferry', exact: true })).toBeVisible();
    expect(new URL(page.url()).pathname).toBe(`/media/${mediaIds.series}`);
    expect(new URL(page.url()).searchParams.get('from')).toBe(submittedSearch);
    await screenshot('native-series-title');
    await returnToOrigin(page, submittedSearch);
    await page.getByRole('link', { name: 'Home', exact: true }).first().click();
    await expect(page.getByRole('heading', { name: 'Continue watching', exact: true })).toBeVisible();
    const menu = page.getByRole('button', { name: 'User menu', exact: true });
    await menu.click();
    await expect(page.getByRole('button', { name: 'Alex', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await expect(page.getByRole('button', { name: 'Morgan', exact: true })).toBeVisible();
    await screenshot('native-profile-menu');
    await page.keyboard.press('Escape');
    await expect(menu).toHaveAttribute('aria-expanded', 'false');
    await expect(menu).toBeFocused();
    return { catalogTitleAndReturn: true, submittedSearchApi: true, searchSeriesTitleAndReturn: true, profileDisclosureEscapeFocus: true, currentProfileId: api.scenario.activeProfileId };
}
