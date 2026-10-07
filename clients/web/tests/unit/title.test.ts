import { describe, expect, it, vi } from 'vitest';
import { collectTitlePages, loadTitleData, loadTitleEpisodes, titleReaders } from '../../src/lib/media/title-data.js';
import { prepareTitlePlayback } from '../../src/lib/media/title-playback.js';
import { selectTitleSeason, selectTitleEpisode, selectedSeasonRoute, titleOrigin, titleDurationMs } from '../../src/lib/media/title-selection.js';

const movie = { id: 'fixture-movie', type: 'movie', title: 'Real fixture title', runtime_seconds: 900 };
const episode = { id: 'fixture-episode', type: 'episode', series_id: 'fixture-series', season_id: 'fixture-season', episode_number: 2 };
const healthyFile = { id: 'healthy-file', is_healthy: true, runtime_seconds: 1200 };
const unhealthyFile = { id: 'unhealthy-file', is_healthy: false, runtime_seconds: 600 };
const watch = { is_watched: false, is_favorite: false, resume_position_ms: 32000 };

function readers(overrides = {}) {
    return {
        ...titleReaders,
        item: vi.fn(async () => movie),
        files: vi.fn(async () => ({ items: [unhealthyFile, healthyFile] })),
        watch: vi.fn(async () => watch),
        seasons: vi.fn(async () => ({ items: [], cursor: null, has_more: false })),
        episodes: vi.fn(async () => ({ items: [], cursor: null, has_more: false })),
        ...overrides,
    };
}

describe('complete Title page reads', () => {
    it('follows every episode cursor and keeps server ordering beyond one page', async () => {
        const items = Array.from({ length: 107 }, (_, index) => ({ ...episode, id: `episode-${index}`, episode_number: index + 1 }));
        const episodes = vi.fn()
            .mockResolvedValueOnce({ items: items.slice(0, 100), cursor: 'next-episodes', has_more: true })
            .mockResolvedValueOnce({ items: items.slice(100), cursor: null, has_more: false });
        const controller = new AbortController();
        const complete = await loadTitleEpisodes('fixture-season', { readers: readers({ episodes }), signal: controller.signal });
        expect(complete).toEqual(items);
        expect(episodes.mock.calls[1]).toEqual(['fixture-season', { limit: 100, cursor: 'next-episodes' }, { signal: controller.signal }]);
    });

    it.each(['duplicate', 'cycle', 'missing-cursor', 'empty-page'])('rejects an incomplete %s traversal', async (kind) => {
        const fetchPage = vi.fn().mockResolvedValueOnce({ items: [{ id: 'first' }], cursor: 'cursor', has_more: true });
        const second = {
            duplicate: { items: [{ id: 'first' }], cursor: null, has_more: false },
            cycle: { items: [{ id: 'second' }], cursor: 'cursor', has_more: true },
            'missing-cursor': { items: [{ id: 'second' }], cursor: null, has_more: true },
            'empty-page': { items: [], cursor: 'next', has_more: true },
        }[kind];
        fetchPage.mockResolvedValueOnce(second);
        await expect(collectTitlePages(fetchPage)).rejects.toMatchObject({ code: 'INCOMPLETE_PAGE' });
    });

    it('cancels delayed page completion before it can become the active season', async () => {
        let finish;
        const fetchPage = vi.fn(() => new Promise((resolve) => { finish = resolve; }));
        const controller = new AbortController();
        const result = collectTitlePages(fetchPage, { signal: controller.signal });
        controller.abort();
        finish({ items: [{ id: 'stale' }], has_more: false });
        await expect(result).rejects.toMatchObject({ name: 'AbortError' });
    });

    it('retains a real title and files when watch state fails', async () => {
        const failure = new Error('Watch unavailable');
        const result = await loadTitleData(movie.id, { readers: readers({ watch: vi.fn().mockRejectedValue(failure) }) });
        expect(result.item).toEqual(movie);
        expect(result.files).toEqual([unhealthyFile, healthyFile]);
        expect(result.watch).toBeNull();
        expect(result.watchError).toBe(failure);
    });

    it('retains series details while reporting an incomplete seasons request', async () => {
        const seasons = vi.fn().mockResolvedValueOnce({ items: [{ id: 'one' }], cursor: 'next', has_more: true }).mockRejectedValueOnce(new Error('Second page denied'));
        const result = await loadTitleData('series', { readers: readers({ item: vi.fn().mockResolvedValue({ id: 'series', type: 'series' }), seasons }) });
        expect(result.item.id).toBe('series');
        expect(result.seasons).toEqual([]);
        expect(result.seasonsError).toBeTruthy();
        expect(result.watch).toEqual(watch);
    });
});

describe('Title selection and origin', () => {
    const seasons = [
        { id: 'specials', season_number: 0, episode_count: 1 },
        { id: 'season-one', season_number: 1, episode_count: 3 },
    ];
    const episodes = [
        { id: 'complete', availability: { can_play: true }, watch_state: { is_watched: true, resume_position_ms: 90000 } },
        { id: 'new', availability: { can_play: true }, watch_state: { is_watched: false, resume_position_ms: 0 } },
        { id: 'unavailable', availability: { can_play: false }, watch_state: { is_watched: false, resume_position_ms: 20000 } },
        { id: 'resume', availability: { can_play: true }, watch_state: { is_watched: false, resume_position_ms: 20000, last_played_at: '2026-10-03' } },
    ];

    it('keeps requested unavailable episodes selectable and never substitutes for invalid IDs', () => {
        expect(selectTitleEpisode(episodes, 'unavailable')?.id).toBe('unavailable');
        expect(selectTitleEpisode(episodes, 'revoked')).toBeNull();
        expect(selectTitleSeason(seasons, 'revoked')).toBeNull();
        expect(selectTitleEpisode(episodes, null)?.id).toBe('resume');
        expect(selectTitleSeason(seasons, null)?.id).toBe('season-one');
    });

    it('preserves complete catalog/search state and discards external origins', () => {
        const origin = '/search?q=harbor&type=series&cursor=next&genre=drama';
        expect(titleOrigin(origin)).toBe(origin);
        expect(titleOrigin('https://outside.invalid')).toBe('/media?type=movie');
        const route = new URL(selectedSeasonRoute({ id: 'fixture-series' }, seasons[1], origin), 'https://duskcue.invalid');
        expect(route.pathname).toBe('/media/fixture-series');
        expect(route.searchParams.get('season')).toBe('season-one');
        expect(route.searchParams.get('from')).toBe(origin);
        expect(route.searchParams.has('episode')).toBe(false);
    });

    it('uses healthy duration and leaves missing duration unknown', () => {
        expect(titleDurationMs(movie, [unhealthyFile, healthyFile])).toBe(1200000);
        expect(titleDurationMs({ duration_ms: 450000 })).toBe(450000);
        expect(titleDurationMs({ runtime_seconds: 0 })).toBeNull();
    });
});

describe('fresh exact-identity Title playback', () => {
    it('re-reads the exact episode and carries the complete Title return URL with a healthy file', async () => {
        const api = readers();
        const destination = '/media/fixture-series?season=fixture-season&episode=fixture-episode&from=%2Fsearch%3Fq%3Dharbor';
        const result = await prepareTitlePlayback(episode, { readers: api, destination });
        expect(api.files).toHaveBeenCalledWith(episode.id, { signal: undefined });
        expect(api.watch).toHaveBeenCalledWith(episode.id, { signal: undefined });
        const route = new URL(result.route, 'https://duskcue.invalid');
        expect(route.pathname).toBe('/play/fixture-episode');
        expect(route.searchParams.get('return_to')).toBe(destination);
        expect(route.searchParams.get('file')).toBe(healthyFile.id);
        expect(result.watch.resume_position_ms).toBe(32000);
    });

    it('rejects an explicit unhealthy file without silently choosing another', async () => {
        await expect(prepareTitlePlayback(movie, { readers: readers(), fileId: unhealthyFile.id })).rejects.toMatchObject({ code: 'FILE_UNAVAILABLE' });
    });

    it('does not launch playback when fresh watch state is unavailable', async () => {
        await expect(prepareTitlePlayback(movie, { readers: readers({ watch: vi.fn().mockRejectedValue(new Error('Revoked')) }) })).rejects.toThrow('Revoked');
    });

    it('discards an external destination and cancels stale playback preparation', async () => {
        const result = await prepareTitlePlayback(movie, { readers: readers(), destination: 'https://outside.invalid' });
        const route = new URL(result.route, 'https://duskcue.invalid');
        expect(route.searchParams.get('return_to')).toBe('/media/fixture-movie');
        const controller = new AbortController();
        const api = readers({ watch: vi.fn(async () => { controller.abort(); return watch; }) });
        await expect(prepareTitlePlayback(movie, { readers: api, signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' });
    });
});
