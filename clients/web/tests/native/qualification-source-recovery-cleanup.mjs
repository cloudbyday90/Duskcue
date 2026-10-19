// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export async function preserveSourceRecoveryFailure(run, restore) {
    let result;
    let failure;
    try { result = await run(); } catch (error) { failure = error; }
    try { await restore(); }
    catch (error) {
        if (failure) throw new AggregateError([failure, error], `${failure.message}; owned recovery preference restoration also failed: ${error.message}`, { cause: failure });
        throw error;
    }
    if (failure) throw failure;
    return result;
}

export function sourceRecoveryReleaseEvidence(records, failures = []) {
    const allocatedSessions = [...new Set(records.filter((record) => record.path === '/api/v1/playback/start' && record.status === 200 && record.reply?.session_id)
        .map((record) => record.reply.session_id))];
    const releasedSessions = records.filter((record) => record.path === '/api/v1/playback/stop' && record.status === 200).map((record) => record.request?.session_id);
    const unreleasedSessions = allocatedSessions.filter((id) => !releasedSessions.includes(id));
    const issues = [...new Set([...failures, ...(unreleasedSessions.length ? ['playback'] : [])])];
    return { passed: issues.length === 0, failures: issues, allocatedSessions, unreleasedSessions,
        clientPlaybackReleased: !issues.includes('playback'), physicalCleanupAuthority: 'isolated-backend-final-cleanup' };
}
