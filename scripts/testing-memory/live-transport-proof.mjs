// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function validateTransportProof(text, resourceId) {
    const records = text.split(/\r?\n/).filter((line) => line.startsWith('LIVE_TRANSPORT_RESULT '));
    if (records.length !== 1 || !text.includes('test result: ok. 1 passed; 0 failed; 0 ignored;')) throw new Error('Exactly one transport contract and proof must pass.');
    const proof = JSON.parse(records[0].slice('LIVE_TRANSPORT_RESULT '.length));
    const checks = ['cookie_and_bearer', 'scope_refusals', 'expired_refusal', 'worker_cleanup', 'upstream_cleanup', 'cache_cleanup', 'blocked_pipe_cancel', 'budget_refusal', 'shutdown_joined', 'ambient_proxy_ignored', 'owner_drop_joined', 'file_budget_refusal', 'eof_refusal'];
    if (proof.version !== 1 || proof.resource_id !== resourceId || checks.some((key) => proof[key] !== true)
        || !Number.isSafeInteger(proof.rotated_sequence) || proof.rotated_sequence < 30
        || !(proof.peak_cache_bytes > 0 && proof.peak_cache_bytes <= 33554432)
        || !(proof.peak_cache_files > 0 && proof.peak_cache_files <= 24)
        || !Number.isFinite(proof.stop_ms) || proof.stop_ms >= 10000 || proof.stop_ms < 0
        || proof.redirect_requests !== 0 || proof.catalog_sessions !== 0
        || !Array.isArray(proof.decoded) || proof.decoded.length !== 2
        || proof.decoded.some(([pixels, frequency]) => pixels !== 640 * 360 * 3 || !Number.isFinite(frequency) || Math.abs(frequency - 997) >= 20)) throw new Error('Transport media, auth, bounds or cleanup proof is incomplete.');
    return proof;
}
