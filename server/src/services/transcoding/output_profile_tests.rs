// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::services::transcoding::arguments::{
    build_ffmpeg_input_args, build_hls_output_args, build_stream_mapping_args_for_video,
};
use crate::services::transcoding::encoding::{build_audio_encode_args, build_video_encode_args};
use crate::services::transcoding::plan::StreamPlan;
use crate::services::transcoding::{TranscodeManager, TranscodeSession};
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

fn params() -> StartSessionParams {
    StartSessionParams {
        media_file_id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        source_path: "movie.mkv".into(),
        source_video_codec: "hevc".into(),
        source_video_resolution: (1920, 1080),
        source_audio_codec: "ac3".into(),
        source_video_stream_index: Some(4),
        source_audio_stream_index: Some(2),
        stream_plan: StreamPlan {
            video: StreamAction::Encode,
            audio: StreamAction::Encode,
        },
        output_profile: Some(OutputProfile::CompatibleSdr),
        target_audio_channels: Some(2),
        target_audio_bitrate: Some(160_000),
        subtitle_stream_index: None,
        subtitle_stream_ordinal: None,
        subtitle_burn_in: false,
        target_video_codec: Some("h264".into()),
        target_audio_codec: Some("aac".into()),
        target_resolution: Some((960, 540)),
        target_bitrate: Some(1_100_000),
        seek_position_ms: Some(12_345),
        hw_accel_override: Some(HwAccelMethod::Software),
        recovery_attempts: 1,
    }
}

#[test]
fn compatible_rendition_preserves_the_admitted_descriptor_without_ladder_rounding() {
    let source = params();
    let rendition = OutputProfile::CompatibleSdr.rendition(&source).unwrap();
    assert_eq!((rendition.width, rendition.height), (960, 540));
    assert_eq!(rendition.video_bitrate, 1_100_000);
    assert_eq!(rendition.audio_bitrate, 160_000);
    assert_eq!(rendition.audio_channels, 2);
    let mut mono = source;
    mono.target_audio_channels = Some(1);
    mono.target_audio_bitrate = Some(AUDIO_MIN_BITRATE);
    mono.target_bitrate = Some(VIDEO_MIN_BITRATE);
    let rendition = OutputProfile::CompatibleSdr.rendition(&mono).unwrap();
    assert_eq!(rendition.audio_channels, 1);
    assert_eq!(rendition.audio_bitrate, AUDIO_MIN_BITRATE);
    assert_eq!(rendition.video_bitrate, VIDEO_MIN_BITRATE);
}

#[test]
fn compatible_profile_refuses_copy_plans_unsupported_codecs_and_hardware() {
    for video in [StreamAction::Copy, StreamAction::Encode] {
        for audio in [StreamAction::Copy, StreamAction::Encode] {
            let mut source = params();
            source.stream_plan = StreamPlan { video, audio };
            assert_eq!(
                OutputProfile::CompatibleSdr.validate(&source).is_ok(),
                video == StreamAction::Encode && audio == StreamAction::Encode
            );
        }
    }
    for hardware in [
        None,
        Some(HwAccelMethod::Nvenc),
        Some(HwAccelMethod::Qsv),
        Some(HwAccelMethod::Vaapi),
        Some(HwAccelMethod::VideoToolbox),
        Some(HwAccelMethod::Amf),
    ] {
        let mut source = params();
        source.hw_accel_override = hardware;
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    for codec in [None, Some("hevc"), Some("av1"), Some("unknown")] {
        let mut source = params();
        source.target_video_codec = codec.map(str::to_owned);
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    for codec in [None, Some("ac3"), Some("opus"), Some("unknown")] {
        let mut source = params();
        source.target_audio_codec = codec.map(str::to_owned);
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
}

#[test]
fn compatible_profile_requires_explicit_even_source_bounded_dimensions() {
    for resolution in [
        None,
        Some((0, 540)),
        Some((960, 0)),
        Some((961, 540)),
        Some((960, 541)),
        Some((MAX_WIDTH + 2, 540)),
        Some((960, MAX_HEIGHT + 2)),
    ] {
        let mut source = params();
        source.target_resolution = resolution;
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    let mut source = params();
    source.source_video_resolution = (640, 360);
    assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    source.target_resolution = Some((640, 360));
    assert!(OutputProfile::CompatibleSdr.validate(&source).is_ok());
    source.source_video_resolution = (0, 0);
    assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    source.source_video_resolution = (MAX_WIDTH, MAX_HEIGHT);
    source.target_resolution = Some((MAX_WIDTH, MAX_HEIGHT));
    assert!(OutputProfile::CompatibleSdr.validate(&source).is_ok());
}

#[test]
fn compatible_profile_rejects_missing_zero_and_out_of_envelope_bitrates_and_channels() {
    for bitrate in [
        None,
        Some(0),
        Some(VIDEO_MIN_BITRATE - 1),
        Some(VIDEO_MAX_BITRATE + 1),
    ] {
        let mut source = params();
        source.target_bitrate = bitrate;
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    for bitrate in [
        None,
        Some(0),
        Some(AUDIO_MIN_BITRATE - 1),
        Some(AUDIO_MAX_BITRATE + 1),
    ] {
        let mut source = params();
        source.target_audio_bitrate = bitrate;
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    for channels in [None, Some(0), Some(AUDIO_MAX_CHANNELS + 1)] {
        let mut source = params();
        source.target_audio_channels = channels;
        assert!(OutputProfile::CompatibleSdr.validate(&source).is_err());
    }
    let mut source = params();
    source.target_bitrate = Some(VIDEO_MAX_BITRATE);
    source.target_audio_bitrate = Some(AUDIO_MAX_BITRATE);
    assert!(OutputProfile::CompatibleSdr.validate(&source).is_ok());
}

fn value<'a>(args: &'a [String], option: &str) -> &'a str {
    let index = args.iter().position(|argument| argument == option).unwrap();
    &args[index + 1]
}

#[test]
fn compatible_output_options_follow_input_and_enforce_the_probed_codecs_and_rates() {
    let source = params();
    let profile = source.output_profile.unwrap();
    let rendition = profile.rendition(&source).unwrap();
    let mut args = build_ffmpeg_input_args(source.seek_position_ms, &source.source_path);
    args.extend(build_stream_mapping_args_for_video(Some(4), Some(2), None));
    args.extend(source.stream_plan.codec_args(
        build_video_encode_args(
            "libx264",
            "h264",
            rendition.width,
            rendition.height,
            rendition.video_bitrate,
            FRAME_RATE * 2,
            HwAccelMethod::Software,
        ),
        build_audio_encode_args("aac", rendition.audio_channels, rendition.audio_bitrate),
    ));
    args.extend(profile.output_args(&source));
    args.extend(build_hls_output_args(
        2,
        "/cache/seg_%04d.m4s",
        "/cache/manifest.m3u8",
    ));
    let input = args.iter().position(|argument| argument == "-i").unwrap();
    for option in ["-level:v", "-r", "-fps_mode:v", "-profile:a", "-ar"] {
        assert!(args.iter().position(|argument| argument == option).unwrap() > input + 1);
        assert_eq!(
            args.iter().filter(|argument| *argument == option).count(),
            1
        );
    }
    for (option, expected) in [
        ("-c:v:0", "libx264"),
        ("-profile:v", "high"),
        ("-level:v", "4.0"),
        ("-pix_fmt", "yuv420p"),
        ("-r", "30"),
        ("-fps_mode:v", "cfr"),
        ("-g", "60"),
        ("-maxrate", "1100000"),
        ("-c:a:0", "aac"),
        ("-profile:a", "aac_low"),
        ("-ar", "48000"),
        ("-ac", "2"),
        ("-b:a", "160000"),
    ] {
        assert_eq!(value(&args, option), expected);
    }
    let video = args.iter().position(|argument| argument == "-map").unwrap();
    assert_eq!(&args[video..video + 4], ["-map", "0:4", "-map", "0:2"]);
    assert!(!args.iter().any(|argument| argument == "copy"));
    let ordinary = build_audio_encode_args("aac", 6, 320_000);
    assert!(
        !ordinary
            .iter()
            .any(|argument| argument == "-profile:a" || argument == "-ar")
    );
    let ordinary = build_video_encode_args(
        "libx264",
        "h264",
        320,
        180,
        200_000,
        48,
        HwAccelMethod::Software,
    );
    assert!(
        !ordinary
            .iter()
            .any(|argument| argument == "-level:v" || argument == "-r")
    );
}

#[tokio::test]
async fn incompatible_profile_is_rejected_before_capacity_or_process_allocation() {
    let mut config = crate::state::RuntimeConfig::default();
    config.cpu.hw_accel_auto_detect = false;
    config.resource_limits.max_concurrent_transcodes = 1;
    let manager = TranscodeManager::new(Arc::new(arc_swap::ArcSwap::from_pointee(config)));
    let _slot = manager.semaphore.try_acquire().unwrap();
    let mut source = params();
    source.stream_plan.audio = StreamAction::Copy;
    let result = manager
        .start_session(source, Path::new("unallocated-output-profile-cache"))
        .await;
    assert!(matches!(
        result,
        Err(crate::domains::playback::PlaybackError::FfmpegFailed(_))
    ));
    assert_eq!(manager.active_session_count(), 0);
}

#[test]
fn restart_keeps_the_profile_targets_and_attempts_for_seek_and_recovery() {
    let source = params();
    let session = TranscodeSession {
        id: Uuid::now_v7(),
        media_file_id: source.media_file_id,
        user_id: source.user_id,
        started_at: chrono::Utc::now(),
        source_path: source.source_path.clone(),
        source_video_codec: source.source_video_codec.clone(),
        source_video_resolution: source.source_video_resolution,
        source_audio_codec: source.source_audio_codec.clone(),
        source_video_stream_index: source.source_video_stream_index,
        source_audio_stream_index: source.source_audio_stream_index,
        stream_plan: source.stream_plan,
        output_profile: source.output_profile,
        target_audio_channels: source.target_audio_channels,
        target_audio_bitrate: source.target_audio_bitrate,
        subtitle_stream_index: Some(8),
        subtitle_stream_ordinal: Some(1),
        subtitle_burn_in: true,
        target_video_codec: source.target_video_codec.clone().unwrap(),
        target_video_resolution: source.target_resolution.unwrap(),
        target_audio_codec: source.target_audio_codec.clone().unwrap(),
        target_bitrate: source.target_bitrate.unwrap(),
        hw_accel: HwAccelMethod::Software,
        source_start_ms: 12_345,
        recovery_attempts: 1,
        rendition_name: "compatible-sdr".into(),
        segment_dir: "cache".into(),
        manifest_path: "cache/manifest.m3u8".into(),
        segments_written: 0,
        client_position_segment: 0,
        progress: None,
        is_complete: false,
        is_seeking: false,
    };
    let mut budget = crate::services::transcoding::recovery::RecoveryBudget::default();
    let action = budget
        .reserve(super::super::FailureClass::Stalled, &session, 45_000)
        .unwrap();
    let mut restart = session.restart_params(action.position_ms);
    restart.recovery_attempts = budget.attempts;
    assert_eq!(restart.output_profile, Some(OutputProfile::CompatibleSdr));
    assert_eq!(restart.seek_position_ms, Some(45_000));
    assert_eq!(restart.source_video_stream_index, Some(4));
    assert_eq!(restart.source_audio_stream_index, Some(2));
    assert_eq!(restart.subtitle_stream_index, Some(8));
    assert_eq!(restart.subtitle_stream_ordinal, Some(1));
    assert!(restart.subtitle_burn_in);
    assert_eq!(restart.recovery_attempts, 2);
    assert_eq!(restart.target_resolution, source.target_resolution);
    assert_eq!(restart.target_bitrate, source.target_bitrate);
    assert_eq!(restart.target_audio_bitrate, source.target_audio_bitrate);
    assert_eq!(restart.target_audio_channels, source.target_audio_channels);
    assert_eq!(restart.hw_accel_override, Some(HwAccelMethod::Software));
    assert!(OutputProfile::CompatibleSdr.validate(&restart).is_ok());
}
