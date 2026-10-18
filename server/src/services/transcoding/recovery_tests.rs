// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::services::transcoding::plan::StreamPlan;
use uuid::Uuid;

fn session(hw_accel: HwAccelMethod, video: StreamAction) -> TranscodeSession {
    TranscodeSession {
        id: Uuid::now_v7(),
        media_file_id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        started_at: chrono::Utc::now(),
        source_path: "movie.mkv".into(),
        source_video_codec: "h264".into(),
        source_video_resolution: (1920, 1080),
        source_audio_codec: "dts".into(),
        source_video_stream_index: Some(4),
        source_audio_stream_index: Some(0),
        stream_plan: StreamPlan {
            video,
            audio: StreamAction::Encode,
        },
        target_audio_channels: Some(6),
        target_audio_bitrate: Some(320_000),
        subtitle_stream_index: Some(8),
        subtitle_stream_ordinal: Some(1),
        subtitle_burn_in: true,
        target_video_codec: "h264".into(),
        target_video_resolution: (1920, 1080),
        target_audio_codec: "aac".into(),
        target_bitrate: 6_000_000,
        hw_accel,
        source_start_ms: 90_000,
        recovery_attempts: 0,
        rendition_name: "1080p".into(),
        segment_dir: "cache".into(),
        manifest_path: "cache/manifest.m3u8".into(),
        segments_written: 0,
        client_position_segment: 0,
        progress: None,
        is_complete: false,
        is_seeking: false,
    }
}

#[test]
fn hardware_fallback_preserves_confirmed_position_and_is_attempt_local() {
    let current = session(HwAccelMethod::Nvenc, StreamAction::Encode);
    let mut budget = RecoveryBudget::default();
    let action = budget
        .reserve(FailureClass::Hardware, &current, 12_345)
        .unwrap();
    assert!(action.software);
    assert_eq!(action.position_ms, 12_345);
    assert_eq!(action.skipped_ms, 0);
    assert!(!action.quality_reduced);
    assert_eq!(current.hw_accel, HwAccelMethod::Nvenc);
    assert_eq!(budget.attempts, 1);
}

#[test]
fn a_stall_can_restart_once_but_cannot_loop_or_skip_content() {
    let current = session(HwAccelMethod::Software, StreamAction::Copy);
    let mut budget = RecoveryBudget::default();
    let action = budget
        .reserve(FailureClass::Stalled, &current, 4_321)
        .unwrap();
    assert_eq!(action.position_ms, 4_321);
    assert_eq!(action.skipped_ms, 0);
    assert!(
        budget
            .reserve(FailureClass::Stalled, &current, 4_321)
            .is_none()
    );
    assert!(budget.exhausted);
    assert_eq!(budget.attempts, 1);
}

#[test]
fn startup_attempt_and_later_seek_cannot_reset_the_total_budget() {
    let mut current = session(HwAccelMethod::Software, StreamAction::Encode);
    current.recovery_attempts = 1;
    let mut budget = RecoveryBudget::default();
    assert!(
        budget
            .reserve(FailureClass::Stalled, &current, 20_000)
            .is_some()
    );
    assert_eq!(budget.attempts, MAX_RECOVERY_ATTEMPTS);
    let persisted = serde_json::to_value(&budget).unwrap();
    let mut after_seek: RecoveryBudget = serde_json::from_value(persisted).unwrap();
    assert!(
        after_seek
            .reserve(FailureClass::Decode, &current, 40_000)
            .is_none()
    );
    assert!(after_seek.exhausted);
}

#[test]
fn decoder_skip_is_bounded_and_requires_an_already_qualified_video_encode() {
    let mut current = session(HwAccelMethod::Software, StreamAction::Encode);
    current.source_start_ms = 0;
    current.progress = Some(crate::services::transcoding::ProgressUpdate {
        frame: 120,
        fps: 24.0,
        bitrate_kbps: 400.0,
        total_size: 1024,
        out_time_ms: 5_000,
        speed: 1.0,
        is_complete: false,
    });
    let mut budget = RecoveryBudget::default();
    let action = budget
        .reserve(FailureClass::Decode, &current, 3_000)
        .unwrap();
    assert_eq!(action.position_ms, 7_000);
    assert_eq!(action.skipped_ms, DECODE_SKIP_MS);
    assert!(
        budget
            .reserve(FailureClass::Decode, &current, 7_000)
            .is_none()
    );
    let copied = session(HwAccelMethod::Software, StreamAction::Copy);
    assert!(
        RecoveryBudget::default()
            .reserve(FailureClass::Decode, &copied, 3_000)
            .is_none()
    );
    assert!(
        RecoveryBudget::default()
            .reserve(FailureClass::Decode, &current, i64::from(i32::MAX))
            .is_none()
    );
}

#[test]
fn container_unknown_and_repeated_software_hardware_failure_do_not_retry() {
    let current = session(HwAccelMethod::Software, StreamAction::Encode);
    for failure in [
        FailureClass::Container,
        FailureClass::Unknown,
        FailureClass::Hardware,
    ] {
        let mut budget = RecoveryBudget::default();
        assert!(budget.reserve(failure, &current, 1_000).is_none());
        assert!(budget.exhausted);
        assert_eq!(budget.attempts, 0);
    }
}

#[test]
fn decode_failure_far_ahead_or_without_a_checkpoint_never_skips_healthy_content() {
    let mut current = session(HwAccelMethod::Software, StreamAction::Encode);
    assert!(
        RecoveryBudget::default()
            .reserve(FailureClass::Decode, &current, 1_000)
            .is_none()
    );
    current.progress = Some(crate::services::transcoding::ProgressUpdate {
        frame: 120,
        fps: 24.0,
        bitrate_kbps: 400.0,
        total_size: 1024,
        out_time_ms: 60_000,
        speed: 1.0,
        is_complete: false,
    });
    assert!(
        RecoveryBudget::default()
            .reserve(FailureClass::Decode, &current, 1_000)
            .is_none()
    );
}
