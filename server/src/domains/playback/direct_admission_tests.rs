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
    let candidate = admit(&media, &device, &network, &config, None).unwrap();
    assert_eq!(candidate.video.content_type, direct::VIDEO_CONTENT_TYPE);
    assert_eq!(candidate.audio.content_type, direct::AUDIO_CONTENT_TYPE);
    assert_eq!(
        (candidate.video.width, candidate.video.height),
        (1920, 1080)
    );
    assert!(u64::from(candidate.video.bitrate) + u64::from(candidate.audio.bitrate) <= 4_000_000);
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
    let candidate = admit(&media, &device, &network, &config, None).unwrap();
    assert_eq!((candidate.video.width, candidate.video.height), (320, 180));
    assert!(candidate.video.bitrate + candidate.audio.bitrate <= 320_000);
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
            admit(&media, &device, &network, &config, None).is_none(),
            "case {failure}"
        );
    }
}

#[test]
fn device_network_and_request_limits_reserve_audio_from_the_same_total_budget() {
    for binding in 0..3 {
        let (media, mut device, mut network, config) = fixture();
        let mut requested = None;
        match binding {
            0 => device.max_bitrate_bps = 300_000,
            1 => network.estimated_throughput_bps = Some(375_000),
            _ => requested = Some(300_000),
        }
        let descriptor = admit(&media, &device, &network, &config, requested).unwrap();
        assert_eq!(descriptor.audio.bitrate, 192_000);
        assert!(descriptor.video.bitrate + descriptor.audio.bitrate <= 300_000);
        assert!(descriptor.video.bitrate <= 108_000);
    }
}

#[test]
fn a_low_bitrate_source_is_admitted_only_when_audio_and_minimum_video_both_fit() {
    let (mut media, mut device, network, config) = fixture();
    media.video_bitrate_bps = 4_000;
    media.audio_bitrate_bps = 64_000;
    device.max_bitrate_bps = 68_000;
    let descriptor = admit(&media, &device, &network, &config, None).unwrap();
    assert_eq!(
        (descriptor.video.bitrate, descriptor.audio.bitrate),
        (4_000, 64_000)
    );
    device.max_bitrate_bps = u64::from(VIDEO_MIN_BITRATE) + 64_000;
    let descriptor = admit(&media, &device, &network, &config, None).unwrap();
    assert_eq!(descriptor.video.bitrate, VIDEO_MIN_BITRATE);
    device.max_bitrate_bps -= 1;
    assert!(admit(&media, &device, &network, &config, None).is_none());
    device.max_bitrate_bps = 68_000;
    assert!(admit(&media, &device, &network, &config, Some(64_999)).is_none());
}

#[test]
fn maximum_quality_keeps_device_and_explicit_total_limits_binding() {
    let (media, device, mut network, mut config) = fixture();
    config.quality_mode = QualityMode::Maximum;
    network.estimated_throughput_bps = Some(1);
    let descriptor = admit(&media, &device, &network, &config, Some(300_000)).unwrap();
    assert!(descriptor.video.bitrate + descriptor.audio.bitrate <= 300_000);
    assert!(admit(&media, &device, &network, &config, Some(50_000)).is_none());
}

#[test]
fn recovery_revalidates_saved_total_and_source_limits_instead_of_only_codec_envelope() {
    let (media, device, network, config) = fixture();
    let descriptor = admit(&media, &device, &network, &config, None).unwrap();
    let total = u64::from(descriptor.video.bitrate) + u64::from(descriptor.audio.bitrate);
    let original = PreparedFallback {
        identity: FileIdentity {
            file_id: uuid::Uuid::now_v7(),
            path: "source.mp4".into(),
            bytes: 1,
            updated_at: chrono::Utc::now(),
            modified_ns: "1".into(),
            probe_hash: "a".repeat(64),
        },
        tracks: ResolvedTracks {
            video: 0,
            audio: 2,
            subtitle: None,
            subtitle_ordinal: None,
        },
        limits: AdmissionLimits {
            max_resolution: device.max_resolution,
            max_audio_channels: device.max_audio_channels,
            max_bitrate: total,
            quality_mode: "auto".into(),
            requested_bitrate: Some(total),
        },
        descriptor,
    };
    assert!(within_saved_limits(&original, &media));
    for failure in 0..6 {
        let mut candidate = original.clone();
        let mut source = media.clone();
        match failure {
            0 => candidate.limits.max_bitrate = total - 1,
            1 => candidate.limits.requested_bitrate = Some(total - 1),
            2 => candidate.limits.max_resolution.0 = candidate.descriptor.video.width - 1,
            3 => candidate.limits.max_audio_channels = 1,
            4 => source.video_bitrate_bps = u64::from(candidate.descriptor.video.bitrate) - 1,
            _ => source.audio_bitrate_bps = u64::from(candidate.descriptor.audio.bitrate) - 1,
        }
        assert!(!within_saved_limits(&candidate, &source), "case {failure}");
    }
}
