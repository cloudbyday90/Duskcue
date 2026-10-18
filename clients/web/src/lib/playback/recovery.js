/*
 * Duskcue — Self-hosted media streaming server
 * Copyright (C) 2026-2026 Duskcue Contributors
 * Licensed under AGPL-3.0. See LICENSE for details.
 */

export function playbackRecovery(value) {
    if (!value || !['recovered', 'exhausted'].includes(value.status)
        || !Number.isSafeInteger(value.attempts) || value.attempts < 0
        || !Number.isSafeInteger(value.skipped_ms) || value.skipped_ms < 0
        || typeof value.reason !== 'string' || typeof value.quality_reduced !== 'boolean') return null;
    return { status: value.status, attempts: value.attempts, reason: value.reason, skipped_ms: value.skipped_ms, quality_reduced: value.quality_reduced };
}

export function playbackRecoveryMessage(recovery) {
    if (recovery?.status === 'pending') return 'tonight_player_recovery_pending';
    if (recovery?.status === 'exhausted') return 'tonight_player_recovery_exhausted';
    if (recovery?.status !== 'recovered') return null;
    if (recovery.skipped_ms > 0 && recovery.quality_reduced) return 'tonight_player_recovery_skipped_quality';
    if (recovery.skipped_ms > 0) return 'tonight_player_recovery_skipped';
    return recovery.quality_reduced ? 'tonight_player_recovery_quality' : 'tonight_player_recovery_compatible';
}
