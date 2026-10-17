// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::parse_probe_output_at;
use serde_json::{Value, json};

fn parse(value: Value) -> super::ProbeResult {
    parse_probe_output_at(
        &serde_json::to_vec(&value).unwrap(),
        std::path::Path::new("source.mkv"),
    )
    .unwrap()
}

#[test]
fn preserves_real_stream_indices_languages_and_structured_audio_flags() {
    let probe = parse(json!({
        "format": {"format_name": "matroska,webm", "duration": "1800.4"},
        "streams": [
            {"index": 0, "codec_type": "video", "codec_name": "h264", "width": 1920, "height": 1080, "r_frame_rate": "24000/1001"},
            {"index": 2, "codec_type": "audio", "codec_name": "aac", "channels": 2, "bit_rate": "192000", "tags": {"language": "eng", "title": "English"}, "disposition": {"default": 1, "visual_impaired": 0}},
            {"index": 5, "codec_type": "audio", "codec_name": "aac", "channels": 2, "tags": {"language": "eng", "title": "Described"}, "disposition": {"default": 0, "visual_impaired": 1}}
        ]
    }));
    assert_eq!(probe.runtime_seconds, 1800);
    assert_eq!(probe.video_resolution.as_deref(), Some("1920x1080"));
    assert!((probe.video_frame_rate.unwrap() - 23.976).abs() < 0.001);
    assert_eq!(probe.audio_language.as_deref(), Some("eng"));
    assert_eq!(probe.audio_bitrate, Some(192000));
    let tracks = probe.additional_streams["audio"].as_array().unwrap();
    assert_eq!(tracks[0]["index"], 2);
    assert_eq!(tracks[0]["is_default"], true);
    assert_eq!(tracks[0]["is_audio_description"], false);
    assert_eq!(tracks[1]["index"], 5);
    assert_eq!(tracks[1]["is_audio_description"], true);
    assert_eq!(tracks[1]["disposition"]["visual_impaired"], 1);
}

#[test]
fn missing_metadata_stays_unknown_even_for_description_titles() {
    let probe = parse(json!({"streams": [
        {"index": 1, "codec_type": "audio", "codec_name": "aac", "tags": {"title": "English Audio Description"}},
        {"index": 3, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"title": "English SDH"}}
    ]}));
    let audio = &probe.additional_streams["audio"][0];
    assert_eq!(audio["language"], "und");
    assert!(audio["is_default"].is_null());
    assert!(audio["is_audio_description"].is_null());
    assert!(audio["disposition"].is_null());
    let subtitle = &probe.additional_streams["subtitles"][0];
    assert_eq!(subtitle["is_hearing_impaired"], true);
    assert!(subtitle["disposition"].is_null());
}

#[test]
fn structured_subtitle_flags_do_not_inherit_legacy_title_heuristics() {
    let probe = parse(json!({"streams": [
        {"index": 3, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"language": "eng", "title": "SDH forced"}, "disposition": {"default": 1, "forced": 0, "hearing_impaired": 0, "descriptions": 0}},
        {"index": 7, "codec_type": "subtitle", "codec_name": "subrip", "tags": {"language": "eng"}, "disposition": {"default": 0, "forced": 1, "hearing_impaired": 1, "descriptions": 0}}
    ]}));
    let subtitles = probe.additional_streams["subtitles"].as_array().unwrap();
    assert_eq!(subtitles[0]["is_default"], true);
    assert_eq!(subtitles[0]["is_forced"], true);
    assert_eq!(subtitles[0]["is_hearing_impaired"], true);
    assert_eq!(subtitles[0]["disposition"]["forced"], 0);
    assert_eq!(subtitles[0]["disposition"]["hearing_impaired"], 0);
    assert_eq!(subtitles[1]["disposition"]["hearing_impaired"], 1);
}

#[test]
fn invalid_disposition_values_are_not_reported_as_known_flags() {
    let probe = parse(json!({"streams": [
        {"index": 1, "codec_type": "audio", "codec_name": "aac", "disposition": {"default": 2, "visual_impaired": -1}}
    ]}));
    let audio = &probe.additional_streams["audio"][0];
    assert!(audio["is_default"].is_null());
    assert!(audio["is_audio_description"].is_null());
    assert_eq!(audio["disposition"]["default"], 2);
}

#[test]
fn keeps_chapters_and_text_description_dispositions_separate() {
    let probe = parse(json!({
        "streams": [{"index": 4, "codec_type": "subtitle", "codec_name": "webvtt", "disposition": {"descriptions": 1, "visual_impaired": 1, "captions": 0}}],
        "chapters": [{"id": 0, "start_time": "0.000", "end_time": "30.000", "tags": {"title": "Opening"}}]
    }));
    assert_eq!(
        probe.additional_streams["chapters"][0]["tags"]["title"],
        "Opening"
    );
    assert_eq!(
        probe.additional_streams["subtitles"][0]["disposition"]["descriptions"],
        1
    );
    assert!(
        probe.additional_streams["subtitles"][0]
            .get("is_audio_description")
            .is_none()
    );
    assert!(probe.additional_streams.get("audio").is_none());
}

#[test]
fn rejects_malformed_probe_json_without_creating_tracks() {
    assert!(parse_probe_output_at(b"not json", std::path::Path::new("source.mkv")).is_err());
    assert!(
        parse_probe_output_at(
            br#"{"streams":[{"index":"one","codec_type":"audio"}]}"#,
            std::path::Path::new("source.mkv")
        )
        .is_err()
    );
}
