import { listSeasonEpisodes } from '../api/media.js';
import { collectTitlePages, assertTitleRequestActive } from '../media/title-data.js';

export class NextEpisodeError extends Error {
    constructor(code) {
        super(code);
        this.code = code;
    }
}

function result(kind, reason, episode = null) {
    return { kind, episode, reason };
}

function knownNumber(number) {
    return Number.isInteger(number) && number >= 0;
}

export async function resolveNextEpisode(item, { signal = undefined, readers = { episodes: listSeasonEpisodes } } = {}) {
    assertTitleRequestActive(signal);
    if (item?.type !== 'episode') return result('none', 'not_episode');
    if (!item.id || !item.series_id || !item.season_id || !knownNumber(item.episode_number)) {
        return result('unordered', 'unknown_current_order');
    }
    const episodes = await collectTitlePages((params, options) => readers.episodes(item.season_id, params, options), { signal });
    assertTitleRequestActive(signal);
    let priorNumber = -1;
    let unknownSeen = false;
    for (const episode of episodes) {
        if (episode.type !== 'episode' || episode.season_id !== item.season_id || episode.series_id !== item.series_id) {
            throw new NextEpisodeError('INVALID_EPISODE_SCOPE');
        }
        if (!knownNumber(episode.episode_number)) {
            unknownSeen = true;
            continue;
        }
        if (unknownSeen || episode.episode_number < priorNumber) throw new NextEpisodeError('INVALID_EPISODE_ORDER');
        priorNumber = episode.episode_number;
    }
    const index = episodes.findIndex((episode) => episode.id === item.id);
    if (index < 0) return result('none', 'current_unavailable');
    if (episodes[index].episode_number !== item.episode_number) return result('unordered', 'current_order_changed');
    const next = episodes[index + 1];
    if (!next) return result('none', 'season_complete');
    if (!knownNumber(next.episode_number)) return result('unordered', 'unknown_next_order');
    return result(next.availability?.can_play === true ? 'available' : 'unavailable', next.availability?.can_play === true ? 'next_available' : 'next_unavailable', next);
}
