// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use serde_json::json;

use super::*;

fn fixture() -> (
    ProbeResult,
    Vec<model::FfprobeStream>,
    Vec<model::FfprobeStream>,
) {
    let avcc = [
        1, 100, 0, 40, 255, 225, 0, 5, 0x67, 100, 0, 40, 0xaa, 1, 0, 2, 0x68, 0xee,
    ];
    let video = json!({"index":0,"codec_type":"video","codec_name":"h264","codec_tag_string":"avc3","profile":"High","level":40,"width":320,"height":180,"pix_fmt":"yuv420p","avg_frame_rate":"24000/1001","color_transfer":"bt709","color_primaries":"bt709","color_space":"bt709","color_range":"tv","extradata_size":avcc.len(),"extradata":STANDARD.encode(avcc)});
    let audio = json!({"index":2,"codec_type":"audio","codec_name":"aac","profile":"LC","sample_rate":"48000","channels":2,"bit_rate":"64000","extradata_size":2,"extradata":STANDARD.encode([0x11,0x90])});
    let result = super::super::parse_probe_output_at(&serde_json::to_vec(&json!({"format":{"format_name":"mov,mp4,m4a,3gp,3g2,mj2","duration":"8.01"},"streams":[video.clone(),audio.clone()]})).unwrap(), Path::new("movie.mp4")).unwrap();
    (
        result,
        vec![serde_json::from_value(video).unwrap()],
        vec![serde_json::from_value(audio).unwrap()],
    )
}

#[test]
fn scanner_descriptor_distinguishes_actual_source_sample_entry_and_copy_output() {
    let (result, video, audio) = fixture();
    let descriptor = describe(
        &result,
        ScanIdentity {
            size: 100000,
            modified_ns: "1000".into(),
        },
        &video,
        &audio,
    )
    .unwrap();
    assert_eq!(descriptor.video.file_codec.as_deref(), Some("avc3.640028"));
    assert_eq!(descriptor.video.copy_codec, "avc1.640028");
    assert_eq!(descriptor.video.frame_rate, 24000.0 / 1001.0);
    assert_eq!(
        descriptor.video.bitrate,
        Some((800000.0f64 / 8.01).ceil() as u64)
    );
    assert_eq!(
        descriptor.video.bitrate_basis.as_deref(),
        Some("container_average")
    );
    assert_eq!(descriptor.audio[0].index, 2);
    assert_eq!(descriptor.audio[0].copy_codec.as_deref(), Some("mp4a.40.2"));
    assert_eq!(descriptor.audio[0].sample_rate, 48000);
    assert_eq!(descriptor.video.config_sha256.len(), 64);
    assert!(
        !serde_json::to_string(&descriptor)
            .unwrap()
            .contains("extradata")
    );
}

#[test]
fn optional_invalid_configuration_never_invalidates_healthy_inventory() {
    let (result, mut video, mut audio) = fixture();
    assert!(super::super::usable_video(&result));
    video[0].extradata_size = Some(65537);
    assert!(
        describe(
            &result,
            ScanIdentity {
                size: 100000,
                modified_ns: "1000".into()
            },
            &video,
            &audio
        )
        .is_none()
    );
    assert!(super::super::usable_video(&result));
    assert_eq!(result.video_codec.as_deref(), Some("h264"));
    let (_, video, _) = fixture();
    audio[0].profile = Some("HE-AAC".into());
    let descriptor = describe(
        &result,
        ScanIdentity {
            size: 100000,
            modified_ns: "1000".into(),
        },
        &video,
        &audio,
    )
    .unwrap();
    assert!(descriptor.audio.is_empty());
}

#[test]
fn changed_configuration_metadata_and_unbounded_dump_fail_closed() {
    let (result, mut video, audio) = fixture();
    video[0].level = Some(51);
    assert!(
        describe(
            &result,
            ScanIdentity {
                size: 100000,
                modified_ns: "1000".into()
            },
            &video,
            &audio
        )
        .is_none()
    );
    let (_, mut video, _) = fixture();
    video[0].extradata = Some("A".repeat(100000));
    assert!(extradata(&video[0]).is_none());
    video[0].extradata = Some(STANDARD.encode([0, 1]));
    assert!(extradata(&video[0]).is_none());
}

#[test]
fn pinned_ffprobe_newline_wrapped_av1_configuration_is_accepted_without_guessing_raw_obus() {
    let stream: model::FfprobeStream = serde_json::from_value(json!({
        "codec_name":"av1", "extradata_size":20, "extradata":"\ngQAMAAoOAAAABDz+zNr5kBAQEEA=\n"
    }))
    .unwrap();
    let bytes = extradata(&stream).unwrap();
    assert_eq!(&bytes[..4], &[0x81, 0, 0x0c, 0]);
    assert_eq!(
        configuration::video("av1", &bytes, 8, [1, 1, 1, 0])
            .unwrap()
            .copy_codec,
        "av01.0.00M.08.0.110.01.01.01.0"
    );
    assert!(configuration::video("av1", &bytes[4..], 8, [1, 1, 1, 0]).is_none());
}

#[test]
fn optional_probe_selects_only_video_or_audio_without_attachment_or_packet_dumps() {
    for selector in ["V", "a"] {
        let args = process::configuration_arguments(selector);
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-select_streams", selector])
        );
        assert!(args.contains(&"-show_data"));
        assert!(!args.contains(&"-show_packets"));
        assert!(!args.contains(&"-show_format"));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["-data_dump_format", "base64"])
        );
    }
    assert_eq!(
        process::ENRICHMENT_DEADLINE,
        std::time::Duration::from_secs(5)
    );
    assert_eq!(process::MAX_STDOUT, 4 * 1024 * 1024);
    assert_eq!(process::MAX_STDERR, 64 * 1024);
}
