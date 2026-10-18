// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function playbackFailureMessage({ recoveryExhausted = false, sourceError = false, preferencesReady = false, error = null } = {}) {
    if (recoveryExhausted) return 'tonight_player_recovery_exhausted';
    if (sourceError) return 'tonight_player_transport_failed';
    if (!preferencesReady) return 'tonight_player_preferences_failed';
    if (error?.status === 422 && error.title === 'PLAY_020') return 'tonight_player_hdr_unsupported';
    return 'lib_components_player_playback_error_the_stream_may_be_unavailable';
}
