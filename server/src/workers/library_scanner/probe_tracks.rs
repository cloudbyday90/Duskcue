// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde_json::{Value, json};

use super::metadata::{known, positive_integer};
use super::model::FfprobeStream;

pub(super) fn flag(value: Option<i64>) -> Option<bool> {
    match value {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    }
}

pub(super) fn audio(stream: &FfprobeStream) -> Option<Value> {
    Some(json!({
        "index": stream.stream_index()?, "codec": known(stream.codec_name.as_deref()),
        "channels": stream.channels.filter(|channels| *channels > 0), "language": stream.language(),
        "title": stream.title(), "bitrate": positive_integer(stream.bit_rate.as_deref()),
        "sample_rate": positive_integer(stream.sample_rate.as_deref()), "channel_layout": known(stream.channel_layout.as_deref()),
        "is_default": stream.disposition.as_ref().and_then(|disposition| flag(disposition.default)),
        "is_audio_description": stream.disposition.as_ref().and_then(|disposition| flag(disposition.visual_impaired)),
        "disposition": stream.disposition,
    }))
}

pub(super) fn subtitle(stream: &FfprobeStream) -> Option<Value> {
    let title_lower = stream.title().unwrap_or_default().to_lowercase();
    let forced = stream
        .disposition
        .as_ref()
        .and_then(|disposition| flag(disposition.forced))
        .unwrap_or(false);
    let hearing_impaired = stream
        .disposition
        .as_ref()
        .and_then(|disposition| flag(disposition.hearing_impaired))
        .unwrap_or(false);
    Some(json!({
        "index": stream.stream_index()?, "codec": known(stream.codec_name.as_deref()), "language": stream.language(), "title": stream.title(),
        "is_forced": forced || title_lower.contains("forced"),
        "is_hearing_impaired": hearing_impaired || title_lower.contains("hearing impaired") || title_lower.contains("sdh") || title_lower.contains("cc"),
        "is_default": stream.disposition.as_ref().and_then(|disposition| flag(disposition.default)),
        "disposition": stream.disposition,
    }))
}
