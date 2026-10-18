// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::services::decision_engine::QualityMode;
use serde_json::json;

fn fixture() -> (
    MediaFileInfo,
    DeviceCapabilities,
    NetworkConditions,
    DecisionEngineConfig,
) {
    let media = MediaFileInfo {
        container_format: "mp4".into(),
        video_codec: "hevc".into(),
        video_profile: Some("Main".into()),
        video_level: Some(4.0),
        video_bit_depth: 8,
        video_resolution: (3840, 2160),
        video_bitrate_bps: 8_000_000,
        video_dynamic_range: "sdr".into(),
        video_frame_rate: 60.0,
        audio_codec: "ac3".into(),
        audio_channels: 6,
        audio_bitrate_bps: 320_000,
        audio_language: Some("eng".into()),
        has_embedded_subtitles: false,
        subtitle_format: None,
        additional_streams: Some(json!({})),
    };
    let device = DeviceCapabilities {
        video_codecs: HashSet::from(["hevc".into()]),
        audio_codecs: HashSet::from(["ac3".into()]),
        containers: HashSet::from(["mp4".into()]),
        subtitle_formats: HashSet::new(),
        max_resolution: (1920, 1080),
        max_audio_channels: 2,
        hdr_formats: HashSet::new(),
        max_bitrate_bps: 4_000_000,
        supports_dolby_vision: false,
        allow_client_side_dv_fallback: false,
        max_video_bit_depth: 10,
    };
    let network = NetworkConditions {
        estimated_throughput_bps: None,
        network_tier: None,
    };
    let config = DecisionEngineConfig {
        default_video_codec: "hevc".into(),
        default_audio_codec: "ac3".into(),
        fallback_max_resolution: (1920, 1080),
        fallback_max_bitrate_bps: 4_000_000,
        throughput_safety_factor: 0.8,
        allow_client_side_dv_fallback: false,
        audio_passthrough_enabled: true,
        subtitle_burn_in_policy: "auto".into(),
        quality_mode: QualityMode::Auto,
        manual_max_resolution: None,
    };
    (media, device, network, config)
}

#[test]
fn compatible_candidate_encodes_both_streams_to_the_exact_software_envelope() {
    let (media, device, network, config) = fixture();
    let candidate = admit(&media, &device, &network, &config).unwrap();
    assert_eq!(candidate.video.content_type, direct::VIDEO_CONTENT_TYPE);
    assert_eq!(candidate.audio.content_type, direct::AUDIO_CONTENT_TYPE);
    assert_eq!(
        (candidate.video.width, candidate.video.height),
        (1920, 1080)
    );
    assert!(candidate.video.bitrate <= 4_000_000);
    assert_eq!(candidate.video.framerate, 30);
    assert_eq!(candidate.audio.channels, "2");
    assert_eq!(candidate.audio.samplerate, 48_000);
    assert!(candidate.audio.bitrate <= 192_000);
}

#[test]
fn lower_source_device_network_and_manual_quality_limits_remain_binding() {
    let (mut media, mut device, mut network, mut config) = fixture();
    media.video_resolution = (640, 360);
    media.video_bitrate_bps = 500_000;
    media.audio_channels = 1;
    media.audio_bitrate_bps = 64_000;
    device.max_resolution = (480, 270);
    device.max_audio_channels = 1;
    config.quality_mode = QualityMode::Manual;
    config.manual_max_resolution = Some((320, 180));
    network.estimated_throughput_bps = Some(400_000);
    let candidate = admit(&media, &device, &network, &config).unwrap();
    assert_eq!((candidate.video.width, candidate.video.height), (320, 180));
    assert!(candidate.video.bitrate <= 320_000);
    assert_eq!(candidate.audio.channels, "1");
    assert_eq!(candidate.audio.bitrate, 64_000);
}

#[test]
fn hdr_dolby_vision_missing_metadata_and_infeasible_caps_are_not_admitted() {
    for failure in 0..6 {
        let (mut media, mut device, network, config) = fixture();
        match failure {
            0 => media.video_dynamic_range = "hdr10".into(),
            1 => media.additional_streams = Some(json!({"dolby_vision":{"profile":8}})),
            2 => media.video_codec = "unknown".into(),
            3 => media.audio_channels = 0,
            4 => device.max_audio_channels = 0,
            _ => device.max_bitrate_bps = 50_000,
        }
        assert!(
            admit(&media, &device, &network, &config).is_none(),
            "case {failure}"
        );
    }
}
