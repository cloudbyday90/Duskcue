import { profileIds } from './catalog.js';

export function accessibleItems(scenario) {
    return scenario.emptyCatalog ? [] : scenario.mediaItems.filter((item) => scenario.activeProfileId !== profileIds.kids || item.content_rating === 'G');
}

export function enrichedItems(scenario) {
    const allowed = accessibleItems(scenario);
    return allowed.map((item) => {
        const children = allowed.filter((child) => child.type === 'episode' && child.season_id === item.id);
        return {
            ...item,
            duration_ms: item.runtime_seconds ? item.runtime_seconds * 1000 : null,
            availability: {
                can_play: scenario.files[item.id].some((file) => file.is_healthy),
                healthy_file_count: scenario.files[item.id].filter((file) => file.is_healthy).length,
            },
            watch_state: scenario.watchByProfile[scenario.activeProfileId][item.id],
            series_title: item.series_id ? 'Harbor Stories' : null,
            ...(item.type === 'season' ? {
                episode_count: children.length,
                available_episode_count: children.filter((child) => scenario.files[child.id].some((file) => file.is_healthy)).length,
                watched_episode_count: children.filter((child) => scenario.watchByProfile[scenario.activeProfileId][child.id].is_watched).length,
            } : {}),
        };
    });
}

export function filterItems(items, query) {
    return items.filter((item) => {
        if (query.get('type') && item.type !== query.get('type')) return false;
        if (query.get('library_id') && item.library_id !== query.get('library_id')) return false;
        if (query.get('watch') === 'watched' && !item.watch_state.is_watched) return false;
        if (query.get('watch') === 'unwatched' && item.watch_state.is_watched) return false;
        if (query.get('watch') === 'in_progress' && (!item.watch_state.resume_position_ms || item.watch_state.is_watched)) return false;
        if (query.has('favorite') && item.watch_state.is_favorite !== (query.get('favorite') === 'true')) return false;
        return true;
    }).sort((left, right) => {
        const sort = query.get('sort');
        const direction = query.get('order') === 'desc' ? -1 : 1;
        if (sort === 'title') return left.sort_title.localeCompare(right.sort_title) * direction;
        if (sort === 'year') return String(left.premiere_date || '').localeCompare(String(right.premiere_date || '')) * direction;
        return 0;
    });
}

export function paginate(items, url, scenario) {
    const query = new URLSearchParams(url.search);
    const cursor = query.get('cursor');
    query.delete('cursor');
    query.delete('limit');
    query.sort();
    const signature = `${scenario.activeProfileId}:${url.pathname}:${query}`;
    let offset = 0;
    if (cursor) {
        try {
            const decoded = JSON.parse(Buffer.from(cursor, 'base64url').toString('utf8'));
            if (decoded.signature !== signature || !Number.isSafeInteger(decoded.offset) || decoded.offset < 0) throw new Error();
            offset = decoded.offset;
        } catch {
            return { status: 422, body: { type: '/errors/validation', title: 'VALID_001', status: 422, detail: 'Fixture cursor scope mismatch' } };
        }
    }
    const limit = Math.min(Number(url.searchParams.get('limit')) || 24, scenario.browsePageSize);
    const next = offset + limit;
    const hasMore = next < items.length;
    return { body: {
        items: items.slice(offset, next),
        cursor: hasMore ? Buffer.from(JSON.stringify({ signature, offset: next })).toString('base64url') : null,
        has_more: hasMore,
    } };
}

export function browsingResponse(scenario, path, url) {
    const items = enrichedItems(scenario);
    const libraries = scenario.libraries.filter((library) => items.some((item) => item.library_id === library.id));
    const collections = scenario.collections.map((collection) => ({
        id: collection.id,
        name: collection.name,
        description: collection.description,
        item_count: items.filter((item) => collection.members.includes(item.id)).length,
        cover_media_item_id: items.find((item) => collection.members.includes(item.id))?.id || null,
    })).filter((collection) => collection.item_count > 0);
    if (path === '/browse/libraries') return paginate(libraries, url, scenario);
    if (path === '/browse/collections') return paginate(collections, url, scenario);
    const metadata = path.match(/^\/browse\/(libraries|collections)\/([^/]+)$/);
    if (metadata) {
        const row = (metadata[1] === 'libraries' ? libraries : collections).find((item) => item.id === metadata[2]);
        return row ? { body: row } : { status: 404, body: { status: 404, detail: 'Fixture browsing metadata unavailable' } };
    }
    const collectionMatch = path.match(/^\/browse\/collections\/([^/]+)\/items$/);
    if (collectionMatch) {
        const collection = scenario.collections.find((entry) => entry.id === collectionMatch[1]);
        return paginate(filterItems(items.filter((item) => collection?.members.includes(item.id)), url.searchParams), url, scenario);
    }
    if (path === '/media-items') return paginate(filterItems(items, url.searchParams), url, scenario);
    if (path === '/media-items/continue-watching') return paginate(items.filter((item) => ['movie', 'episode'].includes(item.type) && item.watch_state.resume_position_ms > 0 && !item.watch_state.is_watched && item.watch_state.last_played_at), url, scenario);
    const parent = path.match(/^\/media-items\/([^/]+)\/(seasons|episodes)$/);
    if (parent) return paginate(items.filter((item) => parent[2] === 'seasons' ? item.type === 'season' && item.series_id === parent[1] : item.type === 'episode' && item.season_id === parent[1]), url, scenario);
    return null;
}
