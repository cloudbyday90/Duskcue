// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use serde_json::json;

fn details() -> MediaFileDetails {
    MediaFileDetails {
        id: Uuid::now_v7(),
        file_path: "movie.mkv".into(),
        container_format: "mkv".into(),
        video_codec: None,
        video_resolution: None,
        video_bitrate: None,
        video_dynamic_range: None,
        video_frame_rate: None,
        audio_codec: None,
        audio_channels: None,
        audio_language: None,
        audio_bitrate: None,
        runtime_seconds: 0,
        additional_streams: json!({}),
    }
}

#[test]
fn absent_source_metadata_does_not_invent_supported_codecs_or_bit_depth() {
    let (media, selected) = build_media_file_info(&details(), None, None).unwrap();
    assert_eq!(media.video_codec, "unknown");
    assert_eq!(media.audio_codec, "unknown");
    assert_eq!(media.video_bit_depth, 0);
    assert_eq!(media.video_frame_rate, 0.0);
    assert_eq!(media.video_dynamic_range, "unknown");
    assert!(selected.video_stream_index.is_none());
}

#[test]
fn normalized_video_inventory_and_global_audio_zero_survive_entry_selection() {
    let mut source = details();
    source.video_codec = Some("hevc".into());
    source.additional_streams = json!({
        "video": {"index":4,"bit_depth":10,"profile":"Main 10","level":5.1,"frame_rate":59.94},
        "audio": [{"index":0,"codec":"ac3","channels":6}]
    });
    let (media, selected) = build_media_file_info(&source, Some(0), None).unwrap();
    assert_eq!(selected.video_stream_index, Some(4));
    assert_eq!(selected.audio_stream_index, Some(0));
    assert_eq!(media.video_bit_depth, 10);
    assert_eq!(media.video_profile.as_deref(), Some("Main 10"));
    assert_eq!(media.video_frame_rate, 59.94);
    assert_eq!(media.audio_codec, "ac3");
    assert_eq!(media.audio_channels, 6);
    assert!(matches!(
        build_media_file_info(&source, Some(1), None),
        Err(PlaybackError::TrackUnavailable("audio"))
    ));
}

#[test]
fn invalid_stored_global_indices_use_the_moving_video_fallback() {
    for index in [-1, i64::from(i32::MAX) + 1] {
        let mut source = details();
        source.additional_streams = json!({"video":{"index":index}});
        let (_, selected) = build_media_file_info(&source, None, None).unwrap();
        assert!(selected.video_stream_index.is_none());
    }
}
