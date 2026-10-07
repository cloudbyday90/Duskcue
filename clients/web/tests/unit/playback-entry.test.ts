import { describe, expect, it, vi } from 'vitest';
import { loadPlaybackEntry, playbackTitleDestination } from '../../src/lib/playback/entry.js';

const episode = { id: 'episode-6', type: 'episode', series_id: 'series-1', season_id: 'season-1' };
const healthy = { id: 'file-1', is_healthy: true };

function api(watch = { is_watched: false, resume_position_ms: 120000 }) {
    return {
        item: vi.fn(async (_id: string, _options: unknown) => episode),
        files: vi.fn(async (_id: string, _options: unknown) => ({ items: [healthy, { id: 'bad-file', is_healthy: false }] })),
        watch: vi.fn(async (_id: string, _options: unknown) => watch),
    };
}

describe('actual playback entry', () => {
    it('reads current watch state and preserves the full same-series Title destination', async () => {
        const readers = api();
        const entry = await loadPlaybackEntry(episode.id, { api: readers, returnTo: '/media/series-1?from=%2Fsearch%3Fq%3Dnight' });
        expect(entry.startPositionMs).toBe(120000);
        expect(entry.file.id).toBe(healthy.id);
        const destination = new URL(entry.destination, 'https://duskcue.invalid');
        expect(destination.searchParams.get('from')).toBe('/search?q=night');
        expect(destination.searchParams.get('episode')).toBe(episode.id);
        expect(destination.searchParams.get('season')).toBe(episode.season_id);
        expect(readers.watch).toHaveBeenCalledWith(episode.id, { signal: undefined });
    });

    it('starts completed content at zero despite a stale resume position', async () => {
        const entry = await loadPlaybackEntry(episode.id, { api: api({ is_watched: true, resume_position_ms: 120000 }) });
        expect(entry.startPositionMs).toBe(0);
    });

    it('rejects an explicitly unavailable file instead of changing the selection', async () => {
        await expect(loadPlaybackEntry(episode.id, { api: api(), fileId: 'bad-file' })).rejects.toMatchObject({ code: 'FILES_UNAVAILABLE' });
    });

    it('keeps failed watch reads recoverable rather than guessing a start position', async () => {
        const readers = api();
        readers.watch.mockRejectedValueOnce(new Error('Read failed'));
        await expect(loadPlaybackEntry(episode.id, { api: readers })).rejects.toThrow('Read failed');
    });

    it('does not commit a cancelled entry even when readers finish', async () => {
        const controller = new AbortController();
        controller.abort();
        await expect(loadPlaybackEntry(episode.id, { api: api(), signal: controller.signal })).rejects.toMatchObject({ name: 'AbortError' });
    });

    it.each(['https://outside.invalid/media/series-1', '//outside.invalid/media/series-1', '/dashboard', '/media/other-series'])('rejects a foreign Title return %s', (destination) => {
        expect(playbackTitleDestination(episode, destination)).toBe('/media/series-1?season=season-1&episode=episode-6');
    });

    it('removes a foreign nested origin and replaces stale episode identity', () => {
        const destination = new URL(playbackTitleDestination(episode, '/media/series-1?from=https%3A%2F%2Foutside.invalid&episode=old'), 'https://duskcue.invalid');
        expect(destination.searchParams.has('from')).toBe(false);
        expect(destination.searchParams.get('episode')).toBe(episode.id);
    });
});
