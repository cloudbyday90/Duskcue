// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

package com.duskcue.mobile.ambient

import androidx.media3.common.C
import androidx.media3.common.Format
import androidx.media3.common.Tracks

internal fun ambientSelectedFormats(tracks: Tracks): List<Pair<Int, Format>> = tracks.groups.flatMap { group ->
    (0 until group.length).filter(group::isTrackSelected).map { group.type to group.getTrackFormat(it) }
}

internal fun ambientRequiredFormatsFailure(formats: List<Pair<Int, Format>>): String? {
    formats.forEach { (type, format) -> ambientFormatFailure(type, format)?.let { return it } }
    return if (formats.any { it.first == C.TRACK_TYPE_VIDEO } && formats.any { it.first == C.TRACK_TYPE_AUDIO }) null
    else "missing_required_tracks"
}

internal fun ambientFormatFailure(type: Int, format: Format): String? = when (type) {
    C.TRACK_TYPE_VIDEO -> when {
        format.sampleMimeType != "video/avc" -> "unqualified_video_codec"
        format.height !in 1..1080 || format.width !in 1..1920 -> "unqualified_video_size"
        format.colorInfo != null && format.colorInfo?.colorTransfer !in setOf(C.COLOR_TRANSFER_SDR, Format.NO_VALUE) -> "unqualified_video_color"
        else -> null
    }
    C.TRACK_TYPE_AUDIO -> when {
        format.sampleMimeType != "audio/mp4a-latm" -> "unqualified_audio_codec"
        format.channelCount !in 1..2 -> "unqualified_audio_channels"
        else -> null
    }
    else -> "unqualified_track_type"
}
