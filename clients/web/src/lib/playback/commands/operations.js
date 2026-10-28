// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

import { PROTOCOL, uuid, path, resource, valid } from './schema.js';
import { probeCompatibleHlsOutput } from '../capabilities.js';
import { probeCopyFallback } from '../copy-fallback.js';

export async function configuration(call, record) {
    const result = await call('GET', path(record.start, '/configuration'));
    const value = result.body;
    valid(value?.protocol === PROTOCOL && value.request_id === record.start && uuid(value.output_revision)
        && typeof value.revision === 'string' && /^[1-9]\d{0,18}$/.test(value.revision)
        && ['preparing', 'ready', 'changing', 'unavailable', 'uncertain', 'retiring'].includes(value.state)
        && (value.pending_operation === null || uuid(value.pending_operation)) && typeof value.worker_recovery === 'boolean'
        && typeof result.etag === 'string' && /^"vod-config-[a-f0-9]{64}"$/.test(result.etag));
    return { value, etag: result.etag };
}
export function operation(result, record) {
    const value = result.body;
    valid(value?.protocol === PROTOCOL && value.request_id === record.start && value.operation_id === record.operation
        && ['unknown', 'accepted', 'preparing', 'completed', 'failed', 'retiring', 'cancelled', 'uncertain'].includes(value.status)
        && typeof value.cleanup_complete === 'boolean');
    resource(value.status_url, record.scope.origin, path(record.start, `/operations/${record.operation}`));
    if (result.status === 202) resource(result.location, record.scope.origin, value.status_url);
    return value;
}
export async function recoveryEvidence(config, failure, { probe = probeCompatibleHlsOutput, copy = probeCopyFallback } = {}) {
    if (failure.kind === 'worker') return config.worker_recovery ? { kind: 'worker' } : null;
    const offer = config.recovery_offer;
    if (!offer || offer.output_revision !== failure.output || typeof offer.fingerprint !== 'string' || !/^[a-f0-9]{64}$/.test(offer.fingerprint)) return null;
    if (offer.kind === 'direct' && failure.mode === 'direct' && failure.code === 3 && await probe(offer.descriptor) === 'supported') {
        return { kind: 'native_decode', offer: offer.fingerprint, supported: true, smooth: null };
    }
    if (offer.kind === 'copy' && ['hls-mse', 'native-hls'].includes(failure.mode) && (failure.mode === 'hls-mse' || failure.code === 3)
        && await copy({ ...offer.descriptor, generation: offer.output_revision })) {
        return { kind: 'hls_media', offer: offer.fingerprint, supported: true, smooth: true };
    }
    return null;
}
