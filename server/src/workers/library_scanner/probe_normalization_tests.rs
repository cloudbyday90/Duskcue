// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use serde_json::{Value, json};

use super::{metadata, parse_probe_output_at, process, usable_video};

fn parse(value: Value, name: &str) -> super::ProbeResult {
    parse_probe_output_at(&serde_json::to_vec(&value).unwrap(), Path::new(name)).unwrap()
}

fn inventory(stream: Value) -> Value {
    json!({"format":{"format_name":"matroska,webm","duration":"1800.4"},"streams":[stream]})
}

#[test]
fn attached_pictures_and_secondary_video_cannot_replace_first_moving_video() {
    let result = parse(
        json!({"format":{"format_name":"matroska,webm"},"streams":[
            {"index":0,"codec_type":"audio","codec_name":"aac"},
            {"index":1,"codec_type":"video","codec_name":"mjpeg","width":400,"height":600,"disposition":{"attached_pic":1}},
            {"index":4,"codec_type":"video","codec_name":"hevc","width":3840,"height":2160,"pix_fmt":"yuv420p10le","profile":"Main 10","level":153},
            {"index":7,"codec_type":"video","codec_name":"h264","width":640,"height":360}
        ]}),
        "movie.mkv",
    );
    assert!(usable_video(&result));
    assert_eq!(result.video_codec.as_deref(), Some("hevc"));
    assert_eq!(result.video_resolution.as_deref(), Some("3840x2160"));
    assert_eq!(result.additional_streams["video"]["index"], 4);
    assert_eq!(result.additional_streams["video"]["bit_depth"], 10);
    assert_eq!(result.additional_streams["video"]["profile"], "Main 10");
    assert_eq!(result.additional_streams["video"]["level"], 5.1);
    assert_eq!(result.additional_streams["video"]["level_raw"], 153);
}

#[test]
fn preserves_rational_rates_color_aspect_interlace_and_dolby_side_data() {
    let result = parse(
        inventory(json!({
            "index":3,"codec_type":"video","codec_name":"hevc","width":1920,"height":1080,
            "pix_fmt":"yuv420p10le","bits_per_raw_sample":"10","field_order":"tt",
            "sample_aspect_ratio":"4:3","display_aspect_ratio":"64:27",
            "avg_frame_rate":"24000/1001","r_frame_rate":"30000/1001",
            "color_transfer":"smpte2084","color_primaries":"bt2020","color_space":"bt2020nc","color_range":"tv","chroma_location":"left",
            "side_data_list":[{"side_data_type":"DOVI configuration record","dv_profile":8,"dv_level":6,"dv_bl_signal_compatibility_id":1,"rpu_present_flag":1,"el_present_flag":0,"bl_present_flag":1}]
        })),
        "movie.mkv",
    );
    let video = &result.additional_streams["video"];
    assert_eq!(video["avg_frame_rate"], "24000/1001");
    assert_eq!(video["r_frame_rate"], "30000/1001");
    assert!((result.video_frame_rate.unwrap() - 24000.0 / 1001.0).abs() < 0.000001);
    assert_eq!(video["field_order"], "tt");
    assert_eq!(video["sample_aspect_ratio"], "4:3");
    assert_eq!(video["display_aspect_ratio"], "64:27");
    assert_eq!(video["color_primaries"], "bt2020");
    assert_eq!(video["color_space"], "bt2020nc");
    assert_eq!(video["color_range"], "tv");
    assert_eq!(video["chroma_location"], "left");
    assert_eq!(
        result.video_dynamic_range.as_deref(),
        Some("dolby_vision_p8")
    );
    let dolby = &result.additional_streams["dolby_vision"];
    assert_eq!(dolby["profile"], 8);
    assert_eq!(dolby["compatibility_mode"], "hdr10");
    assert_eq!(dolby["enhancement_layer"], false);
    assert_eq!(dolby["rpu_present"], true);
    assert_eq!(dolby["side_data"], video["side_data"][0]);
}

#[test]
fn missing_or_invalid_video_metadata_stays_unknown_and_is_not_usable() {
    for codec in [Value::Null, json!("unknown"), json!("N/A"), json!("")] {
        let result = parse(
            inventory(
                json!({"index":0,"codec_type":"video","codec_name":codec,"width":1920,"height":1080}),
            ),
            "movie.mkv",
        );
        assert!(!usable_video(&result));
        assert!(result.video_codec.is_none());
        assert!(result.additional_streams["video"]["bit_depth"].is_null());
        assert!(result.video_dynamic_range.is_none());
        assert!(result.video_frame_rate.is_none());
    }
    for index in [Value::Null, json!(-1), json!(i64::MAX)] {
        let result = parse(
            inventory(
                json!({"index":index,"codec_type":"video","codec_name":"h264","width":1920,"height":1080}),
            ),
            "movie.mkv",
        );
        assert!(!usable_video(&result));
    }
    let result = parse(
        inventory(
            json!({"index":0,"codec_type":"video","codec_name":"h264","width":0,"height":1080}),
        ),
        "movie.mkv",
    );
    assert!(!usable_video(&result));
}

#[test]
fn canonical_containers_cannot_be_claimed_by_unrelated_filename_suffixes() {
    for (format, file, expected) in [
        ("matroska,webm", "movie.mkv", "mkv"),
        ("matroska,webm", "movie.webm", "webm"),
        ("webm", "renamed.mkv", "webm"),
        ("matroska", "renamed.webm", "mkv"),
        ("mov,mp4,m4a,3gp,3g2,mj2", "movie.mp4", "mp4"),
        ("mov,mp4,m4a,3gp,3g2,mj2", "movie.mov", "mov"),
        ("avi", "renamed.mp4", "avi"),
        ("mpegts", "renamed.mkv", "ts"),
    ] {
        assert_eq!(
            metadata::canonical_container(Some(format), Path::new(file)),
            expected
        );
    }
    assert_eq!(
        metadata::canonical_container(None, Path::new("movie.mkv")),
        "unknown"
    );
}

#[test]
fn bit_depth_handles_planar_packed_and_raw_sample_formats_without_inventing_defaults() {
    for (pixel_format, raw_bits, expected) in [
        ("yuv420p", Value::Null, json!(8)),
        ("yuv420p10le", Value::Null, json!(10)),
        ("gbrp12be", Value::Null, json!(12)),
        ("p010le", Value::Null, json!(10)),
        ("rgb24", Value::Null, json!(8)),
        ("rgba64le", Value::Null, json!(16)),
        ("unknown", Value::Null, Value::Null),
        ("mystery", json!("12"), json!(12)),
        ("mystery", json!(0), Value::Null),
        ("mystery", json!("N/A"), Value::Null),
    ] {
        let result = parse(
            inventory(
                json!({"index":0,"codec_type":"video","codec_name":"h264","pix_fmt":pixel_format,"bits_per_raw_sample":raw_bits}),
            ),
            "movie.mkv",
        );
        assert_eq!(
            result.additional_streams["video"]["bit_depth"], expected,
            "{pixel_format}"
        );
    }
}

#[test]
fn invalid_rates_durations_and_unknown_stream_indices_never_become_facts() {
    for rate in ["0/0", "1/0", "-24/-1", "NaN", "inf", "24/1/1", "-1", "0"] {
        assert!(metadata::frame_rate(Some(rate)).is_none(), "{rate}");
    }
    let result = parse(
        json!({"format":{"format_name":"matroska,webm","duration":"NaN"},"streams":[
            {"codec_type":"audio","codec_name":"aac"}, {"index":-3,"codec_type":"subtitle","codec_name":"subrip"}
        ]}),
        "movie.mkv",
    );
    assert_eq!(result.runtime_seconds, 0);
    assert!(result.additional_streams.get("audio").is_none());
    assert!(result.additional_streams.get("subtitles").is_none());
    assert!(!usable_video(&result));
}

#[test]
fn ffprobe_error_record_cannot_authorize_a_usable_inventory() {
    let mut value = inventory(
        json!({"index":0,"codec_type":"video","codec_name":"h264","width":1920,"height":1080}),
    );
    value["error"] =
        json!({"code":-1094995529,"string":"Invalid data found when processing input"});
    let result = parse(value, "movie.mkv");
    assert_eq!(result.additional_streams["video"]["index"], 0);
    assert!(!usable_video(&result));
}

#[tokio::test]
async fn output_readers_refuse_bytes_over_the_limit_and_keep_diagnostics_bounded() {
    assert_eq!(
        process::bounded_read(&b"abcd"[..], 4).await.unwrap(),
        b"abcd"
    );
    assert!(process::bounded_read(&b"abcde"[..], 4).await.is_err());
    assert!(process::diagnostic(&vec![b'x'; process::MAX_STDERR]).len() <= 2048);
    assert!(process::diagnostic(b"a\x00b\n").eq("ab\n"));
}

#[test]
fn probe_profiles_have_one_bounded_escalation_and_file_only_protocols() {
    assert_eq!(process::PROFILES.len(), 2);
    assert_eq!(
        process::PROFILES
            .iter()
            .map(|profile| profile.timeout.as_secs())
            .sum::<u64>(),
        17
    );
    for profile in process::PROFILES {
        let args = process::arguments(profile);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-protocol_whitelist", "file"])
        );
        assert!(!args.contains(&"-nostdin"));
    }
}
