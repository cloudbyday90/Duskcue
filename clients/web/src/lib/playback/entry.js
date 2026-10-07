/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { getMediaItem, listMediaFiles } from '../api/media.js';
import { getWatchData } from '../api/playback.js';
import { isAllowedDesktopRoute, titleRoute } from '../navigation/routes.js';

export class PlaybackEntryError extends Error {
    constructor(code) {
        super(code);
        this.code = code;
    }
}

export function playbackTitleDestination(item, supplied = '') {
    const canonical = new URL(titleRoute(item), 'https://duskcue.invalid');
    if (!isAllowedDesktopRoute(supplied)) return `${canonical.pathname}${canonical.search}`;
    const destination = new URL(supplied, canonical);
    if (destination.pathname !== canonical.pathname) return `${canonical.pathname}${canonical.search}`;
    const origin = destination.searchParams.get('from');
    if (origin && !isAllowedDesktopRoute(origin)) destination.searchParams.delete('from');
    if (item.type === 'episode') {
        if (item.season_id) destination.searchParams.set('season', item.season_id);
        destination.searchParams.set('episode', item.id);
    }
    return `${destination.pathname}${destination.search}`;
}

const readers = { item: getMediaItem, files: listMediaFiles, watch: getWatchData };

export async function loadPlaybackEntry(itemId, { fileId = '', returnTo = '', signal = undefined, api = readers } = {}) {
    const [item, result, watch] = await Promise.all([
        api.item(itemId, { signal }), api.files(itemId, { signal }), api.watch(itemId, { signal }),
    ]);
    if (signal?.aborted) throw new DOMException('Playback request cancelled', 'AbortError');
    if (!item || item.id !== itemId || !['movie', 'episode'].includes(item.type)) throw new PlaybackEntryError('UNPLAYABLE');
    const files = result?.items ?? result;
    if (!Array.isArray(files)) throw new PlaybackEntryError('FILES_UNAVAILABLE');
    const file = fileId ? files.find((candidate) => candidate.id === fileId && candidate.is_healthy === true)
        : files.find((candidate) => candidate.is_healthy === true);
    if (!file) throw new PlaybackEntryError('FILES_UNAVAILABLE');
    const position = Number(watch?.resume_position_ms);
    return {
        item, file, watch,
        startPositionMs: watch?.is_watched || !Number.isFinite(position) ? 0 : Math.max(0, position),
        destination: playbackTitleDestination(item, returnTo),
    };
}
