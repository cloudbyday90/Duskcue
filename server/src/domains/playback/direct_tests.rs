// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::domains::playback::types::{
    DirectFallbackAudio, DirectFallbackVideo, StartPlaybackRequest,
};

#[test]
fn direct_recovery_requires_explicit_supported_start_protocol() {
    for version in [None, Some(0), Some(1), Some(2), Some(255)] {
        let mut body = json!({"media_item_id":Uuid::now_v7()});
        if let Some(version) = version {
            body["direct_recovery_protocol"] = json!(version);
        }
        let request: StartPlaybackRequest = serde_json::from_value(body).unwrap();
        assert_eq!(request.direct_recovery_protocol, version);
        assert_eq!(
            supports_protocol(request.direct_recovery_protocol),
            version == Some(1)
        );
    }
    let request: StartPlaybackRequest = serde_json::from_value(json!({
        "media_item_id":Uuid::now_v7(),"direct_recovery_protocol":null
    }))
    .unwrap();
    assert!(!supports_protocol(request.direct_recovery_protocol));
    for version in [json!(256), json!(-1), json!("1"), json!(true)] {
        assert!(
            serde_json::from_value::<StartPlaybackRequest>(json!({
                "media_item_id":Uuid::now_v7(),"direct_recovery_protocol":version
            }))
            .is_err()
        );
    }
}

fn descriptor() -> DirectFallbackResponse {
    DirectFallbackResponse {
        video: DirectFallbackVideo {
            content_type: VIDEO_CONTENT_TYPE.into(),
            width: 1280,
            height: 720,
            bitrate: 1_000_000,
            framerate: FRAME_RATE,
        },
        audio: DirectFallbackAudio {
            content_type: AUDIO_CONTENT_TYPE.into(),
            channels: "2".into(),
            bitrate: 128_000,
            samplerate: AUDIO_SAMPLE_RATE,
        },
    }
}

#[test]
fn descriptor_rejects_unsupported_codecs_and_any_unenforced_output_envelope() {
    let original = descriptor();
    assert!(descriptor_valid(&original));
    for change in 0..9 {
        let mut descriptor = original.clone();
        match change {
            0 => descriptor.video.content_type = "video/mp4; codecs=\"hvc1\"".into(),
            1 => descriptor.video.width = 1281,
            2 => descriptor.video.height = 2160,
            3 => descriptor.video.bitrate = VIDEO_MAX_BITRATE + 1,
            4 => descriptor.video.framerate = 60,
            5 => descriptor.audio.content_type = "audio/mp4; codecs=\"ac-3\"".into(),
            6 => descriptor.audio.channels = "6".into(),
            7 => descriptor.audio.bitrate = 1,
            _ => descriptor.audio.samplerate = 44_100,
        }
        assert!(!descriptor_valid(&descriptor));
    }
}

#[test]
fn new_direct_revision_rejects_missing_stale_and_pre_seek_hints_without_opt_in_for_legacy() {
    let mut metadata = json!({});
    assert!(!has_context(&metadata));
    assert!(rotate(&mut metadata).is_none());
    let first = Uuid::now_v7();
    attach(
        &mut metadata,
        &DirectContext {
            version: 1,
            candidate: None,
        },
        first,
    );
    assert!(has_context(&metadata));
    assert!(current_revision(&metadata, Some(first)));
    assert!(!current_revision(&metadata, None));
    assert!(!current_revision(&metadata, Some(Uuid::now_v7())));
    let second = rotate(&mut metadata).unwrap();
    assert_ne!(first, second);
    assert!(!current_revision(&metadata, Some(first)));
    assert!(current_revision(&metadata, Some(second)));
    metadata["transcode_session_id"] = json!(Uuid::now_v7());
    assert!(revision(&metadata).is_none());
    assert!(!current_revision(&metadata, Some(second)));
}

#[test]
fn malformed_unknown_version_and_oversized_private_contexts_cannot_offer_fallback() {
    for value in [
        json!(null),
        json!({"version":2,"candidate":null}),
        json!({"version":1,"candidate":{}}),
        json!({"padding":"x".repeat(16_385)}),
    ] {
        let metadata =
            json!({"direct_source_revision":Uuid::now_v7(),"direct_source_context":value});
        assert!(has_context(&metadata));
        assert!(context(&metadata).is_none());
        assert!(super::descriptor(&metadata).is_none());
    }
}

#[tokio::test]
async fn exact_identity_detects_probe_and_physical_source_changes() -> anyhow::Result<()> {
    let directory = std::env::temp_dir().join(format!("duskcue-direct-source-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&directory).await?;
    let path = directory.join("source.bin");
    tokio::fs::write(&path, b"original").await?;
    let mut details = MediaFileDetails {
        id: Uuid::now_v7(),
        file_size: 8,
        updated_at: Utc::now(),
        file_path: path.to_string_lossy().into_owned(),
        container_format: "mp4".into(),
        video_codec: Some("h264".into()),
        video_resolution: Some("320x180".into()),
        video_bitrate: Some(400_000),
        video_dynamic_range: Some("sdr".into()),
        video_frame_rate: None,
        audio_codec: Some("aac".into()),
        audio_channels: Some(2),
        audio_language: None,
        audio_bitrate: Some(128_000),
        runtime_seconds: 1,
        additional_streams: json!({"video":{"index":1},"audio":[{"index":2,"codec":"aac"}]}),
    };
    let initial = FileIdentity::read(&details).await.unwrap();
    details.additional_streams["video"]["index"] = json!(3);
    assert_ne!(initial, FileIdentity::read(&details).await.unwrap());
    tokio::fs::write(&path, b"changed-size").await?;
    assert!(FileIdentity::read(&details).await.is_none());
    tokio::fs::remove_file(&path).await?;
    tokio::fs::remove_dir(&directory).await?;
    Ok(())
}
