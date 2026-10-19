// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    AUDIO_MAX_BITRATE, AUDIO_MAX_CHANNELS, AUDIO_MIN_BITRATE, AUDIO_SAMPLE_RATE, OutputProfile,
};
use crate::services::transcoding::{
    HwAccelMethod, StartSessionParams, TranscodeRendition, plan::StreamAction,
};

pub(super) fn validate(params: &StartSessionParams) -> Result<(), &'static str> {
    if params.output_profile != Some(OutputProfile::SourceCopySdr)
        || params.stream_plan.video != StreamAction::Copy
        || params.hw_accel_override != Some(HwAccelMethod::Software)
        || !matches!(
            params.source_video_codec.as_str(),
            "h264" | "hevc" | "h265" | "av1"
        )
        || params.source_video_resolution.0 == 0
        || params.source_video_resolution.1 == 0
        || params
            .target_resolution
            .is_some_and(|size| size != params.source_video_resolution)
        || params.subtitle_burn_in
    {
        return Err("source-copy SDR output requires unchanged supported video without burn-in");
    }
    if params
        .target_video_codec
        .as_ref()
        .is_some_and(|codec| codec != &params.source_video_codec)
    {
        return Err("source-copy SDR output cannot change the video codec");
    }
    match params.stream_plan.audio {
        StreamAction::Copy
            if params.source_audio_codec == "aac"
                && params.target_audio_channels.is_none()
                && params.target_audio_bitrate.is_none() =>
        {
            Ok(())
        }
        StreamAction::Encode
            if params.target_audio_codec.as_deref() == Some("aac")
                && params
                    .target_audio_channels
                    .is_some_and(|channels| (1..=AUDIO_MAX_CHANNELS).contains(&channels))
                && params.target_audio_bitrate.is_some_and(|bitrate| {
                    (AUDIO_MIN_BITRATE..=AUDIO_MAX_BITRATE).contains(&bitrate)
                }) =>
        {
            Ok(())
        }
        _ => Err("source-copy SDR output requires validated AAC copy or bounded AAC encoding"),
    }
}

pub(super) fn rendition(params: &StartSessionParams) -> TranscodeRendition {
    TranscodeRendition {
        name: "source".into(),
        width: params.source_video_resolution.0,
        height: params.source_video_resolution.1,
        video_bitrate: 0,
        audio_bitrate: params.target_audio_bitrate.unwrap_or(AUDIO_MAX_BITRATE),
        audio_channels: params.target_audio_channels.unwrap_or(AUDIO_MAX_CHANNELS),
    }
}

pub(super) fn output_args(params: &StartSessionParams) -> Vec<String> {
    let mut args = Vec::new();
    let tag = match params.source_video_codec.as_str() {
        "h264" => "avc1",
        "hevc" | "h265" => "hev1",
        "av1" => "av01",
        _ => return args,
    };
    args.extend(["-tag:v".into(), tag.into()]);
    if params.stream_plan.audio == StreamAction::Encode {
        args.extend([
            "-profile:a".into(),
            "aac_low".into(),
            "-ar".into(),
            AUDIO_SAMPLE_RATE.to_string(),
        ]);
    }
    args.extend([
        "-hls_segment_options".into(),
        "use_editlist=0:movflags=+frag_discont:avoid_negative_ts=make_non_negative".into(),
    ]);
    args
}

#[cfg(test)]
#[path = "source_copy_profile_tests.rs"]
mod tests;
