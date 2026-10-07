/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

import { listSegments } from '../api/segments.js';
import { getStoryboard } from '../api/storyboards.js';

export function createPlaybackAnnotations({ onChange, segments = listSegments, storyboard = getStoryboard }) {
    let generation = 0;
    let disposed = false;
    return {
        async load(itemId, fileId) {
            const token = ++generation;
            onChange({ segments: [], storyboard: null });
            const results = await Promise.allSettled([segments(itemId), storyboard(itemId, fileId)]);
            if (disposed || token !== generation) return;
            onChange({ segments: results[0].status === 'fulfilled' ? results[0].value?.segments || [] : [], storyboard: results[1].status === 'fulfilled' ? results[1].value : null });
        },
        dispose() { disposed = true; generation += 1; },
    };
}
