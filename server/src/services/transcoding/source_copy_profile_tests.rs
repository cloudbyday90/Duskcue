// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::transcoding::{
    HwAccelMethod, OutputProfile, StartSessionParams,
    plan::{StreamAction, StreamPlan},
};

fn params() -> StartSessionParams {
    StartSessionParams {
        media_file_id: uuid::Uuid::now_v7(),
        user_id: uuid::Uuid::now_v7(),
        source_path: "source.mkv".into(),
        source_video_codec: "hevc".into(),
        source_video_resolution: (3840, 2160),
        source_audio_codec: "opus".into(),
        source_video_stream_index: Some(0),
        source_audio_stream_index: Some(2),
        stream_plan: StreamPlan {
            video: StreamAction::Copy,
            audio: StreamAction::Encode,
        },
        output_profile: Some(OutputProfile::SourceCopySdr),
        target_audio_channels: Some(2),
        target_audio_bitrate: Some(192_000),
        subtitle_stream_index: None,
        subtitle_stream_ordinal: None,
        subtitle_burn_in: false,
        target_video_codec: None,
        target_audio_codec: Some("aac".into()),
        target_resolution: None,
        target_bitrate: None,
        seek_position_ms: Some(5000),
        hw_accel_override: Some(HwAccelMethod::Software),
        recovery_attempts: 0,
    }
}

#[test]
fn copied_hevc_keeps_dimensions_and_only_converted_audio_gets_encoder_options() {
    let mut params = params();
    let profile = OutputProfile::SourceCopySdr;
    let rendition = profile.rendition(&params).unwrap();
    assert_eq!((rendition.width, rendition.height), (3840, 2160));
    assert_eq!(rendition.video_bitrate, 0);
    assert_eq!(
        profile.output_args(&params),
        [
            "-tag:v",
            "hev1",
            "-profile:a",
            "aac_low",
            "-ar",
            "48000",
            "-hls_segment_options",
            "use_editlist=0:movflags=+frag_discont:avoid_negative_ts=make_non_negative"
        ]
    );
    params.source_audio_codec = "aac".into();
    params.stream_plan.audio = StreamAction::Copy;
    params.target_audio_channels = None;
    params.target_audio_bitrate = None;
    assert!(profile.validate(&params).is_ok());
    assert_eq!(
        profile.output_args(&params),
        [
            "-tag:v",
            "hev1",
            "-hls_segment_options",
            "use_editlist=0:movflags=+frag_discont:avoid_negative_ts=make_non_negative"
        ]
    );
    for (codec, tag) in [("h264", "avc1"), ("av1", "av01")] {
        params.source_video_codec = codec.into();
        assert!(profile.validate(&params).is_ok());
        assert_eq!(
            profile.output_args(&params),
            [
                "-tag:v",
                tag,
                "-hls_segment_options",
                "use_editlist=0:movflags=+frag_discont:avoid_negative_ts=make_non_negative"
            ]
        );
    }
}

#[test]
fn source_copy_refuses_opus_copy_encoding_video_burn_in_and_unbounded_audio() {
    let baseline = params();
    let profile = OutputProfile::SourceCopySdr;
    let changes: [fn(&mut StartSessionParams); 9] = [
        |p: &mut StartSessionParams| {
            p.stream_plan.audio = StreamAction::Copy;
        },
        |p: &mut StartSessionParams| {
            p.stream_plan.video = StreamAction::Encode;
        },
        |p: &mut StartSessionParams| {
            p.subtitle_burn_in = true;
        },
        |p: &mut StartSessionParams| {
            p.target_audio_bitrate = None;
        },
        |p: &mut StartSessionParams| {
            p.target_audio_bitrate = Some(192_001);
        },
        |p: &mut StartSessionParams| {
            p.target_audio_channels = Some(6);
        },
        |p: &mut StartSessionParams| {
            p.target_resolution = Some((1920, 1080));
        },
        |p: &mut StartSessionParams| {
            p.target_video_codec = Some("h264".into());
        },
        |p: &mut StartSessionParams| {
            p.hw_accel_override = Some(HwAccelMethod::Nvenc);
        },
    ];
    for change in changes {
        let mut params = baseline.clone();
        change(&mut params);
        assert!(profile.validate(&params).is_err());
    }
}
