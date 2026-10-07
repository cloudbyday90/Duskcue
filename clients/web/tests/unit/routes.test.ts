import { describe, expect, it } from 'vitest';
import { isAllowedDesktopRoute, playRoute, titleRoute } from '../../src/lib/navigation/routes.js';

describe('desktop internal route boundary', () => {
    it.each([
        '/dashboard',
        '/media',
        '/media?type=series',
        '/media/fixture-title?season=fixture-season&episode=fixture-episode',
        '/search?q=sea%20stories&type=series',
        '/collections',
        '/collections/fixture-collection',
        '/libraries/fixture-library',
        '/play/fixture-title?file=fixture-file',
        '/settings/preferences',
        '/auth/link',
    ])('accepts the supported route %s', (route) => {
        expect(isAllowedDesktopRoute(route)).toBe(true);
    });

    it.each([
        'https://outside.invalid/media',
        '//outside.invalid/media',
        '/\\outside.invalid/media',
        '/media\\fixture-title',
        '/media/%2e%2e/admin',
        '/media/%2Fadmin',
        '/media/%41',
        '/media/../admin',
        '/media/./fixture-title',
        '/media/fixture.title',
        '/admin',
        '/admin/users',
        '/search\n',
        '/search?q=raw space',
        '/unknown',
        null,
        {},
    ])('rejects an unsupported or unsafe destination %s', (route) => {
        expect(isAllowedDesktopRoute(route)).toBe(false);
    });
});

describe('title return context', () => {
    it('returns an episode to its series and retains season, episode and search origin', () => {
        const route = titleRoute({ id: 'fixture-episode', type: 'episode', series_id: 'fixture-series', season_id: 'fixture-season' }, '/search?q=harbor');
        const url = new URL(route, 'https://duskcue.invalid');

        expect(url.pathname).toBe('/media/fixture-series');
        expect(url.searchParams.get('season')).toBe('fixture-season');
        expect(url.searchParams.get('episode')).toBe('fixture-episode');
        expect(url.searchParams.get('from')).toBe('/search?q=harbor');
    });

    it('discards an external return destination', () => {
        expect(titleRoute({ id: 'fixture-movie', type: 'movie' }, 'https://outside.invalid/')).toBe('/media/fixture-movie');
    });

    it('carries a validated title destination into playback', () => {
        const destination = '/media/fixture-series?season=fixture-season&episode=fixture-episode';
        const url = new URL(playRoute({ id: 'fixture-episode', type: 'episode' }, '/search?q=harbor', destination), 'https://duskcue.invalid');
        expect(url.pathname).toBe('/play/fixture-episode');
        expect(url.searchParams.get('return_to')).toBe(destination);
    });

    it('replaces an unsafe playback return destination with the actual title', () => {
        const url = new URL(playRoute({ id: 'fixture-movie', type: 'movie' }, '/media?type=movie', '//outside.invalid/title'), 'https://duskcue.invalid');
        expect(url.searchParams.get('return_to')).toBe('/media/fixture-movie?from=%2Fmedia%3Ftype%3Dmovie');
    });
});
