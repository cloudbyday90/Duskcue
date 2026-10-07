import { get } from '../api/core.js';
import { getMediaItem, listMediaFiles, listSeriesSeasons, listSeasonEpisodes } from '../api/media.js';

export class TitleDataError extends Error {
    constructor(code) {
        super(code);
        this.code = code;
    }
}

export function assertTitleRequestActive(signal) {
    if (signal?.aborted) throw new DOMException('Title request cancelled', 'AbortError');
}

export async function collectTitlePages(fetchPage, { signal = undefined } = {}) {
    const items = [];
    const itemIds = new Set();
    const cursors = new Set();
    let cursor;
    while (true) {
        assertTitleRequestActive(signal);
        const result = await fetchPage({ limit: 100, ...(cursor ? { cursor } : {}) }, { signal });
        assertTitleRequestActive(signal);
        if (!Array.isArray(result?.items)) throw new TitleDataError('INCOMPLETE_PAGE');
        for (const item of result.items) {
            if (!item?.id || itemIds.has(item.id)) throw new TitleDataError('INCOMPLETE_PAGE');
            itemIds.add(item.id);
            items.push(item);
        }
        if (!result.has_more) return items;
        if (!result.items.length || typeof result.cursor !== 'string' || !result.cursor || cursors.has(result.cursor)) {
            throw new TitleDataError('INCOMPLETE_PAGE');
        }
        cursors.add(result.cursor);
        cursor = result.cursor;
    }
}

export const titleReaders = {
    item: getMediaItem,
    files: listMediaFiles,
    watch: (id, options = {}) => get(`/items/${id}/watch-data`, {}, options),
    seasons: listSeriesSeasons,
    episodes: listSeasonEpisodes,
};

export async function loadTitleWatchFiles(itemId, { signal = undefined, readers = titleReaders } = {}) {
    const [filesResult, watchResult] = await Promise.allSettled([
        readers.files(itemId, { signal }),
        readers.watch(itemId, { signal }),
    ]);
    assertTitleRequestActive(signal);
    const files = filesResult.status === 'fulfilled' ? filesResult.value?.items ?? filesResult.value ?? [] : [];
    return {
        files: Array.isArray(files) ? files : [],
        filesError: filesResult.status === 'rejected' ? filesResult.reason : null,
        watch: watchResult.status === 'fulfilled' ? watchResult.value : null,
        watchError: watchResult.status === 'rejected' ? watchResult.reason : null,
    };
}

export async function loadTitleData(itemId, { signal = undefined, readers = titleReaders } = {}) {
    const item = await readers.item(itemId, { signal });
    assertTitleRequestActive(signal);
    const [details, seasonsResult] = await Promise.all([
        loadTitleWatchFiles(itemId, { signal, readers }),
        item.type === 'series'
            ? collectTitlePages((params, options) => readers.seasons(itemId, params, options), { signal })
                .then((seasons) => ({ seasons, seasonsError: null }))
                .catch((seasonsError) => ({ seasons: [], seasonsError }))
            : Promise.resolve({ seasons: [], seasonsError: null }),
    ]);
    assertTitleRequestActive(signal);
    return { item, ...details, ...seasonsResult };
}

export function loadTitleEpisodes(seasonId, { signal = undefined, readers = titleReaders } = {}) {
    return collectTitlePages((params, options) => readers.episodes(seasonId, params, options), { signal });
}
