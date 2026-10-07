/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { isAllowedDesktopRoute, titleRoute } from '../navigation/routes.js';

export function titleOrigin(value, itemType = 'movie') {
    return isAllowedDesktopRoute(value) ? value : `/media?type=${itemType === 'series' ? 'series' : 'movie'}`;
}

export function selectTitleSeason(seasons, requestedId) {
    if (requestedId) return seasons.find((season) => season.id === requestedId) ?? null;
    return seasons.find((season) => season.season_number > 0 && season.episode_count > 0) ?? seasons[0] ?? null;
}

export function selectTitleEpisode(episodes, requestedId) {
    if (requestedId) return episodes.find((episode) => episode.id === requestedId) ?? null;
    const resumable = episodes.filter((episode) => episode.availability?.can_play && !episode.watch_state?.is_watched && episode.watch_state?.resume_position_ms > 0);
    resumable.sort((left, right) => Date.parse(right.watch_state?.last_played_at ?? '') - Date.parse(left.watch_state?.last_played_at ?? ''));
    return resumable[0] ?? episodes.find((episode) => episode.availability?.can_play && !episode.watch_state?.is_watched)
        ?? episodes.find((episode) => episode.availability?.can_play) ?? episodes[0] ?? null;
}

export function selectedSeasonRoute(series, season, origin) {
    return titleRoute({ ...season, type: 'season', series_id: series.id, season_id: season.id }, origin);
}

export function titleDurationMs(item, files = []) {
    const healthyRuntime = files.find((file) => file.is_healthy === true && file.runtime_seconds > 0)?.runtime_seconds;
    if (healthyRuntime) return healthyRuntime * 1000;
    if (item?.duration_ms > 0) return item.duration_ms;
    return item?.runtime_seconds > 0 ? item.runtime_seconds * 1000 : null;
}
