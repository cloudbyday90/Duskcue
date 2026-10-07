import { describe, expect, it } from 'vitest';
import { browsingUrl, catalogQuery, collectionBrowseRoute, collectionListOrigin, collectionQuery, currentOrigin, libraryBrowseRoute, libraryListOrigin, paginationDepth, searchQuery } from '../../src/lib/browsing/query.js';

describe('catalog URL contracts', () => {
    it('retains only a safe collection-list destination across detail navigation', () => {
        expect(collectionListOrigin('/collections?pages=2')).toBe('/collections?pages=2');
        for (const value of ['https://outside.invalid', '//outside.invalid', '/admin', '/collections/child', '/collections/../collections', null]) expect(collectionListOrigin(value)).toBe('/collections');
        expect(new URL(collectionBrowseRoute('owned', '/collections?pages=2'), 'https://fixture.invalid').searchParams.get('from')).toBe('/collections?pages=2');
    });

    it('retains library-list pagination without accepting management or external destinations', () => {
        expect(libraryListOrigin('/libraries?pages=2')).toBe('/libraries?pages=2');
        for (const value of ['/settings/libraries', '/admin', '/libraries/child', '//outside.invalid']) expect(libraryListOrigin(value)).toBe('/libraries');
        expect(new URL(libraryBrowseRoute('owned', '/libraries?pages=2'), 'https://fixture.invalid').searchParams.get('from')).toBe('/libraries?pages=2');
    });
    it('normalizes supported filters and keeps explicit false favorite state', () => {
        expect(catalogQuery(new URLSearchParams('type=series&sort=title&watch=unwatched&favorite=false&library_id=fixture-library'))).toEqual({
            library_id: 'fixture-library', type: 'series', sort: 'title', order: 'asc', watch: 'unwatched', favorite: false,
        });
    });

    it('uses validated defaults and a route-owned library scope', () => {
        expect(catalogQuery(new URLSearchParams('type=unknown&sort=unknown&order=unknown&watch=unknown&favorite=maybe&library_id=other'), { libraryId: 'owned' })).toEqual({
            library_id: 'owned', sort: 'added', order: 'desc', watch: 'all',
        });
    });

    it('changes filters without losing query context and resets pagination', () => {
        const original = new URL('https://fixture.invalid/media?type=movie&favorite=true&pages=3&cursor=old&library_id=owned');
        const next = new URL(browsingUrl(original, { type: 'series', favorite: '' }), original);
        expect(next.pathname).toBe('/media');
        expect(Object.fromEntries(next.searchParams)).toEqual({ type: 'series', library_id: 'owned' });
        expect(original.searchParams.get('pages')).toBe('3');
    });

    it('stores pagination depth while preserving safely encoded search origin', () => {
        const original = new URL('https://fixture.invalid/search?q=sea%20%26%20sun&sort=title');
        const next = new URL(browsingUrl(original, { pages: 2 }, false), original);
        expect(next.searchParams.get('q')).toBe('sea & sun');
        expect(next.searchParams.get('pages')).toBe('2');
        expect(currentOrigin(next)).toBe('/search?q=sea+%26+sun&sort=title&pages=2');
    });
});

describe('pagination and search inputs', () => {
    it('preserves collection membership ordering without adding a catalog library scope', () => {
        expect(collectionQuery(new URLSearchParams('library_id=other&sort=position&order=desc&favorite=true'))).toEqual({ order: 'asc', watch: 'all', favorite: true });
        expect(collectionQuery(new URLSearchParams('sort=title&order=desc&type=movie'))).toEqual({ sort: 'title', order: 'desc', watch: 'all', type: 'movie' });
    });
    it.each([null, '', '0', '-1', '2.5', 'bad', '9007199254740992'])('uses the first page for invalid restoration depth %s', (value) => {
        expect(paginationDepth(value)).toBe(1);
    });

    it('preserves positive pagination depth', () => {
        expect(paginationDepth('4')).toBe(4);
    });

    it('preserves submitted query and supported search facets', () => {
        expect(searchQuery(new URLSearchParams('q=%20harbor%20&type=series&genre=Drama&year=2026&rating_min=7&watch=watched&favorite=false&sort=title'))).toEqual({
            q: 'harbor', type: 'series', genre: 'Drama', year: '2026', rating_min: '7', watch: 'watched', favorite: false, sort: 'title', order: 'asc',
        });
    });
});
