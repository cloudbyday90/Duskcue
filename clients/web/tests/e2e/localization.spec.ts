import type { Locator } from '@playwright/test';
import { test, expect } from './fixtures';
import fr from '../../messages/fr.json' with { type: 'json' };
import ar from '../../messages/ar.json' with { type: 'json' };
import { mediaIds, profileIds } from '../fixtures/catalog.js';

async function expectLanguage(locator: Locator, language: string) {
    await expect(locator).toBeVisible();
    await expect.poll(() => locator.evaluate((element) => element.closest('[lang]')?.getAttribute('lang'))).toBe(language);
}

for (const [locale, messages] of [['fr', fr], ['ar', ar]] as const) {
    test.describe(`${locale} preview message languages`, () => {
        test('home and title retain translated labels while the new heading is English', async ({ page, api }) => {
            await page.goto(`/${locale}/dashboard`);
            await expect(page.locator('html')).toHaveAttribute('lang', locale);
            await expect(page.locator('html')).toHaveAttribute('dir', locale === 'ar' ? 'rtl' : 'ltr');
            await expectLanguage(page.getByRole('heading', { name: messages.routes_dashboard_page_continue_watching, exact: true }), locale);
            await expectLanguage(page.getByRole('heading', { name: messages.routes_dashboard_page_recently_added, exact: true }), locale);
            await expectLanguage(page.getByRole('heading', { name: 'Your evening starts here.', exact: true }), 'en');
            const nav = page.locator('.nav-links');
            const movies = nav.getByRole('link', { name: messages.routes_search_page_movies, exact: true });
            await expect(movies).toHaveAttribute('href', `/${locale}/media?type=movie`);
            await movies.click();
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/${locale}/media`);
            expect(new URL(page.url()).searchParams.get('type')).toBe('movie');
            const tv = nav.getByRole('link', { name: 'TV', exact: true });
            await expect(tv).toHaveAttribute('href', `/${locale}/media?type=series`);
            await tv.click();
            await expect.poll(() => new URL(page.url()).searchParams.get('type')).toBe('series');
            const collections = nav.getByRole('link', { name: messages.routes_collections_page_collections, exact: true });
            await expect(collections).toHaveAttribute('href', `/${locale}/collections`);
            await collections.click();
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/${locale}/collections`);
            await page.goto(`/${locale}/media/${mediaIds.resumeMovie}`);
            await expectLanguage(page.getByRole('button', { name: messages.routes_media_id_page_resume, exact: true }), locale);
            await expectLanguage(page.getByRole('button', { name: 'Favorite', exact: true }), 'en');
            const title = new URL(`/${locale}/media/${mediaIds.series}`, page.url());
            title.searchParams.set('season', mediaIds.seasonOne);
            title.searchParams.set('episode', mediaIds.episodeOne);
            title.searchParams.set('from', '/search?q=Harbor&genre=Drama');
            await page.goto(title.href);
            await expect(page.locator('#selected-episode-heading')).toContainText('A New Arrival');
            const menu = page.getByRole('button', { name: messages.routes_layout_user_menu, exact: true });
            await menu.click();
            await page.getByRole('button', { name: 'Morgan', exact: true }).click();
            await expect(menu).toContainText('Morgan');
            await expect.poll(() => api.activeProfileId).toBe(profileIds.morgan);
            await expect.poll(() => page.url()).toBe(title.href);
            await expect(page.locator('#selected-episode-heading')).toContainText('A New Arrival');
            await expectLanguage(page.locator('.selected-episode').getByRole('button', { name: messages.routes_media_id_page_play, exact: true }), locale);
        });

        test('submitted search preserves filter translations and identifies English names and help', async ({ page, api }) => {
            await page.goto(`/${locale}/search?q=Cloud&genre=Drama`);
            await expect(page.locator('html')).toHaveAttribute('dir', locale === 'ar' ? 'rtl' : 'ltr');
            await expectLanguage(page.getByRole('main').getByRole('button', { name: messages.routes_search_page_search, exact: true }), locale);
            await expectLanguage(page.getByRole('button', { name: messages.routes_search_page_clear_filters, exact: true }), locale);
            const genre = page.getByLabel(messages.routes_search_page_genre, { exact: true });
            await expectLanguage(genre, locale);
            await expect(genre.getByRole('option', { name: messages.routes_search_page_all_genres, exact: true })).toHaveAttribute('lang', locale);
            await expectLanguage(page.getByRole('searchbox', { name: 'Search your library', exact: true }), 'en');
            await expectLanguage(page.getByText('Enter a title, then press Enter or Search.', { exact: true }), 'en');
            expect(await page.evaluate(() => document.documentElement.scrollWidth <= document.documentElement.clientWidth)).toBe(true);
            const headerSearch = page.getByRole('banner').getByRole('searchbox', { name: messages.lib_components_searchbar_search, exact: true });
            await headerSearch.fill('Harbor');
            await headerSearch.press('Enter');
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/${locale}/search`);
            await expect.poll(() => new URL(page.url()).searchParams.get('q')).toBe('Harbor');
            await expectLanguage(page.getByRole('heading', { name: 'Results for “Harbor”', exact: true }), 'en');
            await expectLanguage(page.getByRole('main').getByRole('button', { name: messages.routes_search_page_search, exact: true }), locale);
        });

        test('preference controls and live status use fallback language while Intl names retain the locale', async ({ page, api }) => {
            await page.goto(`/${locale}/dashboard`);
            await page.getByRole('button', { name: messages.routes_layout_user_menu, exact: true }).click();
            const preferencesLink = page.getByRole('link', { name: 'Viewing preferences', exact: true });
            await expect(preferencesLink).toHaveAttribute('href', `/${locale}/settings/preferences`);
            await preferencesLink.click();
            await expect.poll(() => new URL(page.url()).pathname).toBe(`/${locale}/settings/preferences`);
            await expect(page).toHaveTitle(`${messages.routes_settings_page_settings} · Duskcue`);
            const autoplay = page.getByRole('checkbox', { name: 'Play the next episode automatically', exact: true });
            await expectLanguage(autoplay, 'en');
            const audio = page.getByLabel('Preferred audio language', { exact: true });
            await expectLanguage(audio, 'en');
            await expect(audio.locator('option[value="fr"]')).toHaveAttribute('lang', locale);
            await autoplay.uncheck();
            await page.getByRole('button', { name: 'Save changes', exact: true }).click();
            await expectLanguage(page.getByRole('status').filter({ hasText: 'Changes saved.' }), 'en');
            await expect(page.locator('html')).toHaveAttribute('lang', locale);
            await expect(page.locator('html')).toHaveAttribute('dir', locale === 'ar' ? 'rtl' : 'ltr');
        });
    });
}
