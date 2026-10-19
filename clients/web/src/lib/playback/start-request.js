// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

export function playbackStartRequest(mediaItemId, mediaFileId, options = {}) {
    return {
        media_item_id: mediaItemId,
        media_file_id: mediaFileId,
        direct_recovery_protocol: 1,
        device_profile: options.deviceProfile || null,
        max_streaming_bitrate: options.maxBitrate || null,
        force_transcode: options.forceTranscode || false,
        ...(options.forceHls ? { force_hls: true } : {}),
        quality_mode: options.qualityMode || 'auto',
        audio_stream_index: options.audioStreamIndex ?? null,
        subtitle_stream_index: options.subtitleStreamIndex ?? null,
        ...(options.sourceCapabilities ? { source_capabilities: options.sourceCapabilities } : {}),
    };
}
