import { describe, expect, it, vi } from 'vitest';
import { resolveNextEpisode } from '../../src/lib/playback/next-episode.js';

function episode(number, overrides = {}) {
    return { id: `episode-${number}`, type: 'episode', series_id: 'series', season_id: 'season', episode_number: number, availability: { can_play: true }, ...overrides };
}

function readers(items) {
    return { episodes: vi.fn(async (_seasonId, _params, _options) => ({ items, cursor: null, has_more: false })) };
}

describe('real next episode resolution', () => {
    it('traverses all pages and finds the actual successor beyond the first hundred episodes', async () => {
        const items = Array.from({ length: 107 }, (_, index) => episode(index + 1));
        const api = { episodes: vi.fn()
            .mockResolvedValueOnce({ items: items.slice(0, 100), cursor: 'second-page', has_more: true })
            .mockResolvedValueOnce({ items: items.slice(100), cursor: null, has_more: false }) };
        const controller = new AbortController();
        await expect(resolveNextEpisode(items[103], { readers: api, signal: controller.signal })).resolves.toMatchObject({ kind: 'available', episode: items[104] });
        expect(api.episodes.mock.calls[1]).toEqual(['season', { limit: 100, cursor: 'second-page' }, { signal: controller.signal }]);
    });

    it('stops after the selected season final episode and never requests another season', async () => {
        const api = readers([episode(1), episode(2)]);
        await expect(resolveNextEpisode(episode(2), { readers: api })).resolves.toEqual({ kind: 'none', episode: null, reason: 'season_complete' });
        expect(api.episodes).toHaveBeenCalledTimes(1);
        expect(api.episodes.mock.calls[0][0]).toBe('season');
    });

    it('does not invent episode context for movies', async () => {
        const api = readers([episode(1)]);
        await expect(resolveNextEpisode({ id: 'movie', type: 'movie' }, { readers: api })).resolves.toMatchObject({ kind: 'none', reason: 'not_episode' });
        expect(api.episodes).not.toHaveBeenCalled();
    });

    it.each([null, undefined, '1', -1, 1.5])('rejects unknown current numeric ordering %s before querying', async (number) => {
        const api = readers([episode(1), episode(2)]);
        await expect(resolveNextEpisode(episode(number), { readers: api })).resolves.toMatchObject({ kind: 'unordered', reason: 'unknown_current_order' });
        expect(api.episodes).not.toHaveBeenCalled();
    });

    it('keeps actual identity ordering for duplicate numbers and gaps rather than incrementing a guessed number', async () => {
        const first = episode(1);
        const sameNumber = episode(1, { id: 'second-real-id' });
        const later = episode(4);
        await expect(resolveNextEpisode(first, { readers: readers([first, sameNumber, later]) })).resolves.toMatchObject({ episode: sameNumber });
        await expect(resolveNextEpisode(sameNumber, { readers: readers([first, sameNumber, later]) })).resolves.toMatchObject({ episode: later });
    });

    it('does not substitute another episode when the current identity was removed or reordered', async () => {
        await expect(resolveNextEpisode(episode(2), { readers: readers([episode(1), episode(3)]) })).resolves.toMatchObject({ kind: 'none', reason: 'current_unavailable' });
        await expect(resolveNextEpisode(episode(2), { readers: readers([episode(3, { id: 'episode-2' }), episode(4)]) })).resolves.toMatchObject({ kind: 'unordered', reason: 'current_order_changed' });
    });

    it('keeps an unavailable next episode instead of skipping to a later playable one', async () => {
        const next = episode(2, { availability: { can_play: false } });
        await expect(resolveNextEpisode(episode(1), { readers: readers([episode(1), next, episode(3)]) })).resolves.toEqual({ kind: 'unavailable', episode: next, reason: 'next_unavailable' });
    });

    it('does not authorize advancement into unknown episode order', async () => {
        await expect(resolveNextEpisode(episode(1), { readers: readers([episode(1), episode(null)]) })).resolves.toMatchObject({ kind: 'unordered', episode: null, reason: 'unknown_next_order' });
    });

    it.each([{ series_id: 'other-series' }, { season_id: 'other-season' }, { type: 'movie' }])('rejects foreign identity %j', async (overrides) => {
        await expect(resolveNextEpisode(episode(1), { readers: readers([episode(1), episode(2, overrides)]) })).rejects.toMatchObject({ code: 'INVALID_EPISODE_SCOPE' });
    });

    it.each([[episode(2), episode(1)], [episode(1), episode(null), episode(2)]])('rejects a malformed ordered response', async (...items) => {
        await expect(resolveNextEpisode(episode(1), { readers: readers(items) })).rejects.toMatchObject({ code: 'INVALID_EPISODE_ORDER' });
    });

    it('propagates an incomplete page failure without publishing the first-page candidate', async () => {
        const failure = new Error('Second page unavailable');
        const api = { episodes: vi.fn().mockResolvedValueOnce({ items: [episode(1), episode(2)], cursor: 'more', has_more: true }).mockRejectedValueOnce(failure) };
        await expect(resolveNextEpisode(episode(1), { readers: api })).rejects.toBe(failure);
    });

    it('discards delayed resolution after the active scope is aborted', async () => {
        let finish;
        const api = { episodes: vi.fn(() => new Promise((resolve) => { finish = resolve; })) };
        const controller = new AbortController();
        const pending = resolveNextEpisode(episode(1), { readers: api, signal: controller.signal });
        controller.abort();
        finish({ items: [episode(1), episode(2)], cursor: null, has_more: false });
        await expect(pending).rejects.toMatchObject({ name: 'AbortError' });
    });
});
