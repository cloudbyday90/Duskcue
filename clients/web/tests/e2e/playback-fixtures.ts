import { test as base, expect } from './fixtures';
import { ensurePlaybackMedia } from '../fixtures/playback-media.mjs';
import { installPlaybackApi } from '../fixtures/playback-api.mjs';

type PlaybackOptions = {
    mode?: 'direct_play' | 'transcode';
    clip?: 'standard' | 'ending';
    autoplay?: boolean;
    resumePositionMs?: number;
    startDelayMs?: number;
    seekReplacement?: boolean;
    lostStopAcknowledgements?: number;
    progressiveHls?: boolean;
    segments?: Array<{ id: string; segment_type: string; start_ms: number; end_ms: number; skip_to_ms: number; is_manual: boolean; confidence: number }>;
};

export const test = base.extend<{
    playbackOptions: PlaybackOptions;
    playback: Awaited<ReturnType<typeof installPlaybackApi>>;
}, {
    media: Awaited<ReturnType<typeof ensurePlaybackMedia>>;
}>({
    playbackOptions: [{}, { option: true }],
    media: [async ({}, use) => { await use(await ensurePlaybackMedia()); }, { scope: 'worker' }],
    playback: async ({ page, api, media, playbackOptions }, use) => {
        await use(await installPlaybackApi(page, api, media, playbackOptions));
    },
});

export { expect };
