import { mediaIds } from './catalog.js';
import { enrichedItems, filterItems, paginate } from './browsing.js';

export function searchResponse(scenario, url) {
    if (scenario.profiles.find((profile) => profile.id === scenario.activeProfileId)?.allow_search === false) return { status: 403, body: { status: 403, detail: 'Fixture profile search unavailable' } };
    const query = url.searchParams;
    const term = (query.get('q') || '').trim().toLowerCase();
    const matching = filterItems(enrichedItems(scenario).filter((item) => {
        if (!`${item.title} ${item.overview}`.toLowerCase().includes(term)) return false;
        const genre = item.id === mediaIds.kidsMovie ? 'family' : 'drama';
        if (query.get('genre') && genre !== query.get('genre')) return false;
        if (query.get('year') && item.premiere_date?.slice(0, 4) !== query.get('year')) return false;
        return !query.get('rating_min') || item.rating_average >= Number(query.get('rating_min'));
    }), query);
    const count = (value, label, matches) => ({ value, label, count: matches.length });
    const facets = {
        types: [...new Set(matching.map((item) => item.type))].map((type) => count(type, type, matching.filter((item) => item.type === type))),
        genres: ['drama', 'family'].map((genre) => count(genre, genre === 'drama' ? 'Drama' : 'Family', matching.filter((item) => (item.id === mediaIds.kidsMovie ? 'family' : 'drama') === genre))).filter((facet) => facet.count),
        years: [...new Set(matching.map((item) => item.premiere_date?.slice(0, 4)).filter(Boolean))].map((year) => count(year, year, matching.filter((item) => item.premiere_date?.startsWith(year)))),
        ratings: ['9', '8', '7', '6'].map((rating) => count(rating, `${rating}+`, matching.filter((item) => item.rating_average >= Number(rating)))),
    };
    const response = paginate(matching, url, scenario);
    return response.status ? response : { body: { ...response.body, facets } };
}
