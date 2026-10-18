// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { describe, expect, it } from 'vitest';
import { ApiError } from '../../src/lib/api/core.js';
import { playbackFailureMessage } from '../../src/lib/playback/failure-message.js';

describe('playback failure messages', () => {
    const error = new ApiError({ status: 422, title: 'PLAY_020', detail: 'Untrusted server detail' });
    const ready = { preferencesReady: true, error };
    it('uses a localized key for the exact typed refusal without rendering server detail', () => {
        expect(playbackFailureMessage(ready)).toBe('tonight_player_hdr_unsupported');
        for (const error of [null, new Error('PLAY_020'), { title: 'PLAY_020', status: 500 }, { title: 'PLAY_004', status: 422 }]) {
            expect(playbackFailureMessage({ ...ready, error })).toBe('lib_components_player_playback_error_the_stream_may_be_unavailable');
        }
    });
    it('retains recovery, transport and preference failures in their existing priority', () => {
        expect(playbackFailureMessage({ ...ready, recoveryExhausted: true, sourceError: true })).toBe('tonight_player_recovery_exhausted');
        expect(playbackFailureMessage({ ...ready, sourceError: true })).toBe('tonight_player_transport_failed');
        expect(playbackFailureMessage({ ...ready, preferencesReady: false })).toBe('tonight_player_preferences_failed');
    });
});
