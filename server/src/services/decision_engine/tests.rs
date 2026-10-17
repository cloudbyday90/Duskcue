// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use super::*;

fn sdr_media() -> MediaFileInfo {
    MediaFileInfo {
        container_format: "matroska".to_string(),
        video_codec: "h264".to_string(),
        video_profile: Some("High".to_string()),
        video_level: Some(4.1),
        video_bit_depth: 8,
        video_resolution: (1920, 1080),
        video_bitrate_bps: 8_000_000,
        video_dynamic_range: "sdr".to_string(),
        video_frame_rate: 23.976,
        audio_codec: "aac".to_string(),
        audio_channels: 2,
        audio_bitrate_bps: 192_000,
        audio_language: Some("eng".to_string()),
        has_embedded_subtitles: false,
        subtitle_format: None,
        additional_streams: None,
    }
}

fn basic_device() -> DeviceCapabilities {
    DeviceCapabilities {
        video_codecs: HashSet::from(["h264".to_string()]),
        audio_codecs: HashSet::from(["aac".to_string()]),
        containers: HashSet::from(["mp4".to_string(), "mkv".to_string()]),
        subtitle_formats: HashSet::from(["srt".to_string(), "webvtt".to_string()]),
        max_resolution: (1920, 1080),
        max_audio_channels: 2,
        hdr_formats: HashSet::new(),
        max_bitrate_bps: 20_000_000,
        supports_dolby_vision: false,
        allow_client_side_dv_fallback: true,
        max_video_bit_depth: 8,
    }
}

fn default_config() -> DecisionEngineConfig {
    DecisionEngineConfig {
        default_video_codec: "h264".to_string(),
        default_audio_codec: "aac".to_string(),
        fallback_max_resolution: (3840, 2160),
        fallback_max_bitrate_bps: 6_000_000,
        throughput_safety_factor: 0.8,
        allow_client_side_dv_fallback: true,
        audio_passthrough_enabled: true,
        subtitle_burn_in_policy: "last_resort".to_string(),
        quality_mode: QualityMode::Auto,
        manual_max_resolution: None,
    }
}

fn good_network() -> NetworkConditions {
    NetworkConditions {
        estimated_throughput_bps: Some(50_000_000),
        network_tier: Some("excellent".to_string()),
    }
}

#[test]
fn test_direct_play_h264_aac_mkv() {
    let decision = decide(
        &sdr_media(),
        &basic_device(),
        &good_network(),
        &default_config(),
    );
    assert_eq!(decision.overall, StreamDecision::DirectPlay);
    assert_eq!(decision.video, VideoDecision::DirectPlay);
    assert_eq!(decision.audio, AudioDecision::Passthrough);
}

#[test]
fn test_transcode_unsupported_video_codec() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.overall, StreamDecision::Transcode);
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_video_codec, Some("h264".to_string()));
}

#[test]
fn test_transcode_resolution_exceeds_device() {
    let mut media = sdr_media();
    media.video_resolution = (3840, 2160);
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((1920, 1080)));
}

#[test]
fn test_tone_map_hdr_to_sdr() {
    let mut media = sdr_media();
    media.video_dynamic_range = "hdr10".to_string();
    media.video_bit_depth = 10;
    media.video_codec = "hevc".to_string();
    let mut device = basic_device();
    device.video_codecs.insert("hevc".to_string());
    device.max_video_bit_depth = 10;
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);
    assert!(decision.requires_tone_mapping);
    assert_eq!(decision.target_video_codec, Some("h264".to_string()));
}

#[test]
fn test_audio_transcode_unsupported_codec() {
    let mut media = sdr_media();
    media.audio_codec = "truehd".to_string();
    media.audio_channels = 8;
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.audio, AudioDecision::Transcode);
    assert_eq!(decision.target_audio_codec, Some("aac".to_string()));
    assert_eq!(decision.target_audio_channels, Some(2));
}

#[test]
fn test_audio_downmix() {
    let mut media = sdr_media();
    media.audio_channels = 6;
    let mut device = basic_device();
    device.audio_codecs.insert("aac".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.audio, AudioDecision::Downmix);
}

#[test]
fn test_subtitle_passthrough_srt() {
    let mut media = sdr_media();
    media.subtitle_format = Some("srt".to_string());
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.subtitle, SubtitleDecision::Passthrough);
}

#[test]
fn test_subtitle_burn_in_pgs() {
    let mut media = sdr_media();
    media.subtitle_format = Some("pgs".to_string());
    media.video_codec = "h264".to_string();
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.subtitle, SubtitleDecision::BurnIn);
    assert!(decision.subtitle_burn_in_required);
}

#[test]
fn test_subtitle_convert_ass_to_srt() {
    let mut media = sdr_media();
    media.subtitle_format = Some("ass".to_string());
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.subtitle, SubtitleDecision::Convert);
}

#[test]
fn test_dv_profile7_client_fallback() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    media.video_dynamic_range = "dolby_vision_p7".to_string();
    media.video_bit_depth = 10;
    media.additional_streams = Some(serde_json::json!({
        "dolby_vision": { "profile": 7, "level": 6, "compatibility_mode": "hdr10" }
    }));
    let mut device = basic_device();
    device.video_codecs.insert("hevc".to_string());
    device.max_video_bit_depth = 10;
    device.hdr_formats.insert("hdr10".to_string());
    device.allow_client_side_dv_fallback = true;
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::DirectPlay);
}

#[test]
fn test_dv_profile7_no_fallback_strip() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    media.video_dynamic_range = "dolby_vision_p7".to_string();
    media.video_bit_depth = 10;
    media.additional_streams = Some(serde_json::json!({
        "dolby_vision": { "profile": 7, "level": 6 }
    }));
    let mut device = basic_device();
    device.video_codecs.insert("hevc".to_string());
    device.max_video_bit_depth = 10;
    device.hdr_formats.insert("hdr10".to_string());
    device.allow_client_side_dv_fallback = false;
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Remux);
    assert!(decision.requires_dv_strip);
}

#[test]
fn test_dv_profile5_must_tone_map_without_native_support() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    media.video_dynamic_range = "dolby_vision_p5".to_string();
    media.video_bit_depth = 10;
    media.additional_streams = Some(serde_json::json!({
        "dolby_vision": { "profile": 5, "level": 6 }
    }));
    let mut device = basic_device();
    device.video_codecs.insert("hevc".to_string());
    device.max_video_bit_depth = 10;
    device.hdr_formats.insert("hdr10".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);
    assert!(decision.requires_tone_mapping);
}

#[test]
fn test_maximum_quality_mode() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    let mut device = basic_device();
    device.video_codecs.insert("hevc".to_string());
    let mut config = default_config();
    config.quality_mode = QualityMode::Maximum;
    let decision = decide(&media, &device, &good_network(), &config);
    assert_eq!(decision.overall, StreamDecision::DirectPlay);
    assert_eq!(decision.video, VideoDecision::DirectPlay);
}

#[test]
fn test_bitrate_exceeds_network() {
    let mut media = sdr_media();
    media.video_bitrate_bps = 50_000_000;
    media.audio_bitrate_bps = 5_000_000;
    let device = basic_device();
    let network = NetworkConditions {
        estimated_throughput_bps: Some(20_000_000),
        network_tier: Some("good".to_string()),
    };
    let decision = decide(&media, &device, &network, &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);
}

#[test]
fn test_codec_alias_matching() {
    let supported: HashSet<String> = HashSet::from(["h264".to_string()]);
    assert!(codec_supported("h264", &supported));
    assert!(codec_supported("avc", &supported));
    assert!(codec_supported("avc1", &supported));
    assert!(!codec_supported("hevc", &supported));
}

#[test]
fn test_remux_container_mismatch() {
    let mut media = sdr_media();
    media.container_format = "matroska".to_string();
    let mut device = basic_device();
    device.containers = HashSet::from(["mp4".to_string()]);
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Remux);
    assert_eq!(decision.overall, StreamDecision::DirectStream);
}

#[test]
fn test_10bit_exceeds_device() {
    let mut media = sdr_media();
    media.video_bit_depth = 10;
    let device = basic_device();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);
}

#[test]
fn test_parse_resolution_string() {
    assert_eq!(parse_resolution_string("4k"), (3840, 2160));
    assert_eq!(parse_resolution_string("1080p"), (1920, 1080));
    assert_eq!(parse_resolution_string("720p"), (1280, 720));
    assert_eq!(parse_resolution_string("480p"), (854, 480));
    assert_eq!(parse_resolution_string("unknown"), (1920, 1080));
}

#[test]
fn test_normalize_resolution() {
    assert_eq!(normalize_resolution(3840, 2160), (3840, 2160));
    assert_eq!(normalize_resolution(2560, 1440), (1920, 1080));
    assert_eq!(normalize_resolution(1920, 1080), (1920, 1080));
    assert_eq!(normalize_resolution(1280, 720), (1280, 720));
    assert_eq!(normalize_resolution(640, 480), (640, 480));
}

#[test]
fn test_audio_passthrough_disabled() {
    let mut config = default_config();
    config.audio_passthrough_enabled = false;
    let decision = decide(&sdr_media(), &basic_device(), &good_network(), &config);
    assert_eq!(decision.audio, AudioDecision::Transcode);
}

#[test]
fn test_subtitle_pgs_when_supported() {
    let mut media = sdr_media();
    media.subtitle_format = Some("pgs".to_string());
    let mut device = basic_device();
    device.subtitle_formats.insert("pgs".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.subtitle, SubtitleDecision::Passthrough);
}

#[test]
fn container_support_matches_exact_formats_and_complete_probe_families() {
    for (container, supported, expected) in [
        ("matroska", vec!["mkv"], VideoDecision::DirectPlay),
        (" MKV ", vec!["Matroska"], VideoDecision::DirectPlay),
        ("mp4", vec!["mp4"], VideoDecision::DirectPlay),
        ("webm", vec!["webm"], VideoDecision::DirectPlay),
        ("mpegts", vec!["ts"], VideoDecision::DirectPlay),
        ("m2ts", vec!["mpegts"], VideoDecision::DirectPlay),
        ("avi", vec!["avi"], VideoDecision::DirectPlay),
        ("avi", vec!["mkv"], VideoDecision::Remux),
        ("flv", vec!["mkv"], VideoDecision::Remux),
        ("webm", vec!["mkv"], VideoDecision::Remux),
        ("matroska", vec!["webm"], VideoDecision::Remux),
        ("mov", vec!["mp4"], VideoDecision::Remux),
        ("mp4", vec!["mov"], VideoDecision::Remux),
        ("matroska,webm", vec!["mkv"], VideoDecision::Remux),
        ("matroska,webm", vec!["webm"], VideoDecision::Remux),
        (
            "matroska,webm",
            vec!["mkv", "webm"],
            VideoDecision::DirectPlay,
        ),
        ("mov,mp4,m4a,3gp,3g2,mj2", vec!["mp4"], VideoDecision::Remux),
        (
            "mov,mp4,m4a,3gp,3g2,mj2",
            vec!["mov", "mp4", "m4a", "3gp", "3g2", "mj2"],
            VideoDecision::DirectPlay,
        ),
        (
            "mov,mp4,m4a,3gp,3g2,mj2",
            vec!["mov,mp4,m4a,3gp,3g2,mj2"],
            VideoDecision::DirectPlay,
        ),
        ("unknown", vec!["unknown"], VideoDecision::Remux),
        ("", vec![""], VideoDecision::Remux),
        ("mp4,", vec!["mp4"], VideoDecision::Remux),
    ] {
        let mut media = sdr_media();
        media.container_format = container.to_string();
        let mut device = basic_device();
        device.containers = supported.iter().map(|format| format.to_string()).collect();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, expected, "{container} with {supported:?}");
        assert_eq!(
            decision.overall,
            if expected == VideoDecision::DirectPlay {
                StreamDecision::DirectPlay
            } else {
                StreamDecision::DirectStream
            },
            "{container} with {supported:?}"
        );
    }
}

#[test]
fn popular_video_codecs_use_exact_advertised_support() {
    for (source, supported) in [
        ("avc1", "h264"),
        (" H264 ", " AVC "),
        ("hvc1", "hevc"),
        ("hevc", "hev1"),
        ("av01", "av1"),
        ("vp09", "vp9"),
        ("vp8", "vp8"),
        ("mp4v", "mpeg4"),
        ("mpeg2video", "mpeg2"),
        ("vc-1", "vc1"),
        ("mjpeg", "mjpeg"),
    ] {
        let mut media = sdr_media();
        media.video_codec = source.to_string();
        let mut device = basic_device();
        device.video_codecs = HashSet::from([supported.to_string()]);
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.overall, StreamDecision::DirectPlay, "{source}");

        device.video_codecs.clear();
        let decision = decide(&media, &device, &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::Transcode, "{source}");
    }
}

#[test]
fn unknown_codecs_never_qualify_for_direct_play_even_when_advertised() {
    for codec in ["", "unknown", "none", "n/a", "und", "unspecified"] {
        let mut media = sdr_media();
        media.video_codec = codec.to_string();
        media.audio_codec = codec.to_string();
        let mut device = basic_device();
        device.video_codecs.insert(codec.to_string());
        device.audio_codecs.insert(codec.to_string());
        let mut config = default_config();
        config.quality_mode = QualityMode::Maximum;
        let decision = decide(&media, &device, &good_network(), &config);
        assert_eq!(decision.video, VideoDecision::Transcode, "{codec}");
        assert_eq!(decision.audio, AudioDecision::Transcode, "{codec}");
        assert_eq!(decision.overall, StreamDecision::Transcode, "{codec}");
    }
}

#[test]
fn pcm_sample_formats_do_not_imply_each_others_support() {
    let mut media = sdr_media();
    media.audio_codec = "pcm_s32le".to_string();
    let mut device = basic_device();
    device.audio_codecs = HashSet::from(["pcm_s16le".to_string()]);
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.audio, AudioDecision::Transcode);
    device.audio_codecs.insert("pcm_s32le".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.audio, AudioDecision::Passthrough);
}

#[test]
fn maximum_quality_preserves_hard_device_limits() {
    let mut config = default_config();
    config.quality_mode = QualityMode::Maximum;
    let mut media = sdr_media();
    media.video_bit_depth = 10;
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Transcode);

    media = sdr_media();
    media.video_resolution = (3840, 2160);
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((1920, 1080)));

    media = sdr_media();
    media.video_codec = "hevc".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Transcode);

    media = sdr_media();
    media.container_format = "avi".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Remux);
    assert_eq!(decision.overall, StreamDecision::DirectStream);

    media = sdr_media();
    media.video_dynamic_range = "hdr10".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::ToneMap);
    assert!(decision.requires_tone_mapping);
}

#[test]
fn unknown_video_bit_depth_does_not_qualify_for_direct_play() {
    for mode in [QualityMode::Auto, QualityMode::Maximum] {
        let mut media = sdr_media();
        media.video_bit_depth = 0;
        let mut config = default_config();
        config.quality_mode = mode;
        let decision = decide(&media, &basic_device(), &good_network(), &config);
        assert_eq!(decision.video, VideoDecision::Transcode);
        assert_eq!(decision.overall, StreamDecision::Transcode);
    }
}

#[test]
fn maximum_quality_ignores_bandwidth_preferences_but_not_device_bitrate() {
    let mut config = default_config();
    config.quality_mode = QualityMode::Maximum;
    config.manual_max_resolution = Some((854, 480));
    let network = NetworkConditions {
        estimated_throughput_bps: Some(500_000),
        network_tier: Some("poor".to_string()),
    };
    let media = sdr_media();
    let mut device = basic_device();
    let decision = decide(&media, &device, &network, &config);
    assert_eq!(decision.overall, StreamDecision::DirectPlay);

    device.max_bitrate_bps = 4_000_000;
    let decision = decide(&media, &device, &network, &config);
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert!(
        decision
            .target_bitrate_bps
            .is_some_and(|bitrate| bitrate <= 4_000_000)
    );
    assert_eq!(decision.target_resolution, Some((1920, 1080)));
}

#[test]
fn maximum_quality_checks_audio_codec_and_channel_limits() {
    let mut config = default_config();
    config.quality_mode = QualityMode::Maximum;
    let mut media = sdr_media();
    media.audio_codec = "truehd".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.audio, AudioDecision::Transcode);
    assert_eq!(decision.overall, StreamDecision::Transcode);

    media = sdr_media();
    media.audio_channels = 8;
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.audio, AudioDecision::Downmix);
    assert_eq!(decision.overall, StreamDecision::Transcode);
    assert_eq!(decision.target_audio_codec.as_deref(), Some("aac"));
    assert_eq!(decision.target_audio_channels, Some(2));
}

#[test]
fn unsupported_hdr_format_requires_tone_mapping_despite_other_hdr_support() {
    let mut media = sdr_media();
    media.video_dynamic_range = "hlg".to_string();
    let mut device = basic_device();
    device.hdr_formats.insert("hdr10".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);
    assert!(decision.requires_tone_mapping);

    device.hdr_formats.insert("HLG".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::DirectPlay);
}

#[test]
fn codec_and_bit_depth_failures_do_not_hide_required_tone_mapping() {
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    media.video_bit_depth = 10;
    media.video_dynamic_range = "hdr10".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);
    assert!(decision.requires_tone_mapping);
    assert_eq!(decision.target_video_codec.as_deref(), Some("h264"));
}

#[test]
fn unknown_dynamic_range_transcodes_without_claiming_hdr_tone_mapping() {
    for dynamic_range in ["unknown", "", "unspecified", "unrecognized"] {
        let mut media = sdr_media();
        media.video_dynamic_range = dynamic_range.to_string();
        let mut config = default_config();
        config.quality_mode = QualityMode::Maximum;
        let decision = decide(&media, &basic_device(), &good_network(), &config);
        assert_eq!(decision.video, VideoDecision::Transcode, "{dynamic_range}");
        assert!(!decision.requires_tone_mapping, "{dynamic_range}");
        assert!(
            decision
                .decision_reasons
                .iter()
                .any(|reason| { reason.factor == "dynamic_range" && reason.result == "transcode" })
        );
    }

    for dynamic_range in ["hdr10", "hlg", "hdr10_plus", "dolby_vision_p5"] {
        let mut media = sdr_media();
        media.video_dynamic_range = dynamic_range.to_string();
        let decision = decide(&media, &basic_device(), &good_network(), &default_config());
        assert_eq!(decision.video, VideoDecision::ToneMap, "{dynamic_range}");
        assert!(decision.requires_tone_mapping, "{dynamic_range}");
    }
}

#[test]
fn remux_does_not_bypass_bandwidth_or_manual_resolution_limits() {
    let mut media = sdr_media();
    media.container_format = "avi".to_string();
    let network = NetworkConditions {
        estimated_throughput_bps: Some(2_000_000),
        network_tier: None,
    };
    let decision = decide(&media, &basic_device(), &network, &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);

    let mut config = default_config();
    config.quality_mode = QualityMode::Manual;
    config.manual_max_resolution = Some((1280, 720));
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((1280, 720)));
}

#[test]
fn dv_client_fallback_does_not_bypass_container_or_resolution_limits() {
    let mut media = sdr_media();
    media.video_dynamic_range = "dolby_vision_p8.1".to_string();
    media.additional_streams = Some(serde_json::json!({ "dolby_vision": { "profile": 8 } }));
    media.container_format = "avi".to_string();
    let mut device = basic_device();
    device.hdr_formats.insert("hdr10".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Remux);

    media.video_resolution = (3840, 2160);
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((1920, 1080)));
}

#[test]
fn dv_profile8_requires_known_supported_base_and_native_dv_is_distinct() {
    let mut media = sdr_media();
    media.video_dynamic_range = "dolby_vision_p8".to_string();
    media.additional_streams = Some(serde_json::json!({ "dolby_vision": { "profile": 8 } }));
    let mut device = basic_device();
    device.hdr_formats.insert("hdr10".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);

    media.video_dynamic_range = "dolby_vision_p8.4".to_string();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::ToneMap);
    device.hdr_formats.insert("hlg".to_string());
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::DirectPlay);

    device.supports_dolby_vision = true;
    device.hdr_formats.clear();
    let decision = decide(&media, &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::DirectPlay);
}

#[test]
fn transcode_resolution_does_not_exceed_narrow_or_small_device_bounds() {
    let mut device = basic_device();
    device.max_resolution = (640, 360);
    let decision = decide(&sdr_media(), &device, &good_network(), &default_config());
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((640, 360)));

    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    media.video_resolution = (640, 480);
    let decision = decide(&media, &basic_device(), &good_network(), &default_config());
    assert_eq!(decision.target_resolution, Some((640, 480)));
}

#[test]
fn fallback_profile_resolution_does_not_cap_known_device_capabilities() {
    let mut config = default_config();
    config.fallback_max_resolution = (1280, 720);
    config.quality_mode = QualityMode::Maximum;
    let mut media = sdr_media();
    media.video_codec = "hevc".to_string();
    let decision = decide(&media, &basic_device(), &good_network(), &config);
    assert_eq!(decision.video, VideoDecision::Transcode);
    assert_eq!(decision.target_resolution, Some((1920, 1080)));
}
