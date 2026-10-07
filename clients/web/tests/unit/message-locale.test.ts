import { describe, expect, it } from 'vitest';
import { messageLocale } from '../../src/lib/localization/message-locale.js';
import { incompleteMessages } from '../../src/lib/localization/message-availability.js';
import { buildMessageAvailability, readMessageAvailability } from '../../scripts/message-availability.mjs';

describe('message language follows the compiled catalog fallback', () => {
    it('retains translated labels and identifies new English fallback in preview locales', () => {
        for (const locale of ['fr', 'de', 'es', 'it', 'ar', 'zh-Hans', 'zh-Hant'] as const) {
            expect(messageLocale('tonight_continue_watching', locale)).toBe(locale);
            expect(messageLocale('tonight_title_play', locale)).toBe(locale);
            expect(messageLocale('tonight_player_menu_speed', locale)).toBe(locale);
            expect(messageLocale('tonight_autoplay_countdown_status', locale)).toBe('en');
            expect(messageLocale('tonight_preferences_description', locale)).toBe('en');
            expect(messageLocale('routes_layout_home', locale)).toBe(locale);
        }
        expect(messageLocale('tonight_autoplay_countdown_status', 'en')).toBe('en');
    });

    it('records partial translation availability without shipping translation payloads', () => {
        expect(buildMessageAvailability({ en: { $schema: 'schema', __translator_note: 'note', ready: 'Ready', added: 'Added' }, fr: { ready: 'Prêt', added: 'Ajouté' }, ar: { ready: 'جاهز' } }, ['en', 'fr', 'ar'], 'en')).toEqual({ added: ['en', 'fr'] });
    });

    it('matches every actual catalog and preserves the equivalent preview messages', async () => {
        const { catalogs, locales, baseLocale, incompleteMessages: actual } = await readMessageAvailability();
        expect(incompleteMessages).toEqual(actual);
        for (const locale of locales) for (const key of Object.keys(catalogs[baseLocale]).filter((key) => !key.startsWith('$') && !key.startsWith('__'))) {
            expect(messageLocale(key, locale), `${locale}:${key}`).toBe(Object.hasOwn(catalogs[locale], key) ? locale : baseLocale);
        }
        const preserved = {
            tonight_continue_watching: 'routes_dashboard_page_continue_watching',
            tonight_recently_added: 'routes_dashboard_page_recently_added',
            tonight_title_play: 'routes_media_id_page_play',
            tonight_title_resume: 'routes_media_id_page_resume',
            tonight_browse_search: 'routes_search_page_search',
            tonight_browse_search_submit: 'routes_search_page_search',
            tonight_browse_clear: 'routes_search_page_clear_filters',
            tonight_browse_all_genres: 'routes_search_page_all_genres',
            tonight_browse_all_years: 'routes_search_page_all_years',
            tonight_player_menu_speed: 'lib_components_player_playback_speed',
        };
        for (const locale of locales.filter((value) => value !== baseLocale)) for (const [key, previous] of Object.entries(preserved)) {
            expect(catalogs[locale][key], `${locale}:${key}`).toEqual(catalogs[locale][previous]);
        }
    });
});
