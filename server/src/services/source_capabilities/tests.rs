// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;

fn avcc() -> Vec<u8> {
    vec![
        1, 100, 0, 40, 255, 225, 0, 5, 0x67, 100, 0, 40, 0xaa, 1, 0, 2, 0x68, 0xee,
    ]
}

fn hvcc(depth: u8) -> Vec<u8> {
    let mut bytes = vec![0; 23];
    bytes[0] = 1;
    bytes[1] = if depth == 10 { 2 } else { 1 };
    bytes[2] = 0x60;
    bytes[6] = 0xb0;
    bytes[12] = 60;
    bytes[13] = 0xf0;
    bytes[15] = 0xfc;
    bytes[16] = 0xfd;
    bytes[17] = 0xf8 | (depth - 8);
    bytes[18] = bytes[17];
    bytes[21] = 3;
    bytes[22] = 3;
    for kind in [32, 33, 34] {
        bytes.extend([0x80 | kind, 0, 1, 0, 2, kind << 1, 1]);
    }
    bytes
}

#[test]
fn avc_requires_actual_complete_parameter_records_and_matching_sps_identity() {
    let bytes = avcc();
    assert_eq!(
        configuration::video("h264", &bytes, 8, [1, 1, 1, 0])
            .unwrap()
            .copy_codec,
        "avc1.640028"
    );
    for size in 0..bytes.len() {
        assert!(configuration::video("h264", &bytes[..size], 8, [1, 1, 1, 0]).is_none());
    }
    let mut mismatch = bytes.clone();
    mismatch[10] = 0x40;
    assert!(configuration::video("h264", &mismatch, 8, [1, 1, 1, 0]).is_none());
    assert!(configuration::video("h264", &bytes, 10, [1, 1, 1, 0]).is_none());
    assert!(configuration::video("h264", &[0, 0, 0, 1, 0x67], 8, [1, 1, 1, 0]).is_none());
}

#[test]
fn hevc_retains_compatibility_constraints_tier_level_and_actual_depth() {
    for (depth, expected) in [(8, "hev1.1.6.L60.B0"), (10, "hev1.2.6.L60.B0")] {
        let bytes = hvcc(depth);
        assert_eq!(
            configuration::video("hevc", &bytes, depth, [1, 1, 1, 0])
                .unwrap()
                .copy_codec,
            expected
        );
        for size in 0..bytes.len() {
            assert!(configuration::video("hevc", &bytes[..size], depth, [1, 1, 1, 0]).is_none());
        }
        let mut rext = bytes.clone();
        rext[1] = 4;
        assert!(configuration::video("hevc", &rext, depth, [1, 1, 1, 0]).is_none());
        let mut missing = bytes.clone();
        missing[22] = 2;
        assert!(configuration::video("hevc", &missing, depth, [1, 1, 1, 0]).is_none());
        let mut high = bytes;
        high[1] |= 32;
        high[12] = 120;
        assert!(
            configuration::video("hevc", &high, depth, [1, 1, 1, 0])
                .unwrap()
                .copy_codec
                .contains(".H120.")
        );
    }
}

#[test]
fn av1_requires_real_configuration_and_preserves_nondefault_color() {
    let mut bytes = vec![0x81, 0, 0x0c, 0, 0x0a, 1, 0];
    assert_eq!(
        configuration::video("av1", &bytes, 8, [1, 1, 1, 0])
            .unwrap()
            .copy_codec,
        "av01.0.00M.08.0.110.01.01.01.0"
    );
    bytes[2] |= 64;
    assert_eq!(
        configuration::video("av1", &bytes, 10, [9, 16, 9, 0])
            .unwrap()
            .copy_codec,
        "av01.0.00M.10.0.110.09.16.09.0"
    );
    assert!(configuration::video("av1", &bytes, 8, [1, 1, 1, 0]).is_none());
    assert!(configuration::video("av1", &bytes, 10, [2, 2, 2, 0]).is_none());
    assert!(configuration::video("av1", &bytes[4..], 10, [1, 1, 1, 0]).is_none());
    bytes.extend([0x0a, 1, 0]);
    assert!(configuration::video("av1", &bytes, 10, [1, 1, 1, 0]).is_none());
}

#[test]
fn aac_lc_matches_asc_rate_channels_and_rejects_he_aac_extensions() {
    assert_eq!(
        configuration::audio("aac", &[0x11, 0x90], Some("LC"), 48000, 2),
        Some("mp4a.40.2")
    );
    assert_eq!(
        configuration::audio("aac", &[0x11, 0x90, 0x56, 0xe5, 0], Some("LC"), 48000, 2),
        Some("mp4a.40.2")
    );
    for (bytes, profile, rate, channels) in [
        (&[0x11, 0x90][..], Some("HE-AAC"), 48000, 2),
        (&[0x11, 0x90], Some("LC"), 44100, 2),
        (&[0x11, 0x90], Some("LC"), 48000, 1),
        (&[0x11, 0x90, 0x56, 0xe5, 0x80], Some("LC"), 48000, 2),
        (&[0x11], Some("LC"), 48000, 2),
    ] {
        assert!(configuration::audio("aac", bytes, profile, rate, channels).is_none());
    }
}

fn descriptor() -> Value {
    serde_json::json!({"playback_descriptor": {
        "version":1,"scan":{"size":5000,"modified_ns":"123456789"},"moving_video_count":1,
        "video":{"index":0,"codec":"h264","file_codec":"avc1.640028","copy_codec":"avc1.640028","width":320,"height":180,"bit_depth":8,"frame_rate":24.0,"bitrate":100000,"bitrate_basis":"stream","config_sha256":"a".repeat(64),"dynamic_range":"sdr"},
        "audio":[{"index":2,"codec":"aac","file_codec":"mp4a.40.2","copy_codec":"mp4a.40.2","bitrate":64000,"sample_rate":48000,"channels":2,"config_sha256":"b".repeat(64)}]
    }})
}

#[test]
fn legacy_unknown_malformed_and_ambiguous_descriptors_fail_closed() {
    assert!(PlaybackDescriptor::from_metadata(&descriptor()).is_some());
    assert!(PlaybackDescriptor::from_metadata(&serde_json::json!({})).is_none());
    for (field, bad) in [
        ("version", serde_json::json!(2)),
        ("moving_video_count", serde_json::json!(0)),
    ] {
        let mut value = descriptor();
        value["playback_descriptor"][field] = bad;
        assert!(PlaybackDescriptor::from_metadata(&value).is_none());
    }
    for (field, bad) in [
        ("frame_rate", serde_json::json!(0)),
        ("bit_depth", serde_json::json!(0)),
        ("copy_codec", serde_json::json!("-i secret")),
        ("copy_codec", serde_json::json!("avc1.bad")),
        ("config_sha256", serde_json::json!("bad")),
        ("bitrate_basis", serde_json::json!("guessed")),
    ] {
        let mut value = descriptor();
        value["playback_descriptor"]["video"][field] = bad;
        assert!(PlaybackDescriptor::from_metadata(&value).is_none());
    }
    let mut duplicate = descriptor();
    duplicate["playback_descriptor"]["audio"][0]["index"] = serde_json::json!(0);
    assert!(PlaybackDescriptor::from_metadata(&duplicate).is_none());
}

#[tokio::test]
async fn descriptor_identity_matches_opened_file_and_detects_resize() {
    let root = std::env::temp_dir().join(format!("duskcue-descriptor-{}", uuid::Uuid::now_v7()));
    tokio::fs::create_dir(&root).await.unwrap();
    let path = root.join("owned.bin");
    tokio::fs::write(&path, [1, 2, 3]).await.unwrap();
    let before = ScanSnapshot::read(&path).await.unwrap();
    let file = tokio::fs::File::open(&path).await.unwrap();
    assert!(
        before
            .identity
            .matches_metadata(&file.metadata().await.unwrap())
    );
    tokio::fs::write(&path, [1, 2, 3, 4]).await.unwrap();
    assert_ne!(Some(before), ScanSnapshot::read(&path).await);
    tokio::fs::remove_file(&path).await.unwrap();
    tokio::fs::remove_dir(&root).await.unwrap();
}
