use super::*;

const MEDIA: &str = include_str!("../../../tests/fixtures/hls/ffmpeg_media.m3u8");
const MASTER: &str = include_str!("../../../tests/fixtures/hls/ffmpeg_master.m3u8");

#[test]
fn real_ffmpeg_fmp4_media_references_bind_to_rendition_routes() {
    let session = Uuid::now_v7();
    let normalized = normalize_manifest(MEDIA, session, "remux").unwrap();
    assert!(normalized.contains(&format!(
        "#EXT-X-MAP:URI=\"/api/v1/transcode/{session}/remux/init.mp4\""
    )));
    for index in 0..5 {
        assert!(normalized.contains(&format!(
            "/api/v1/transcode/{session}/remux/seg_{index:04}.m4s\n"
        )));
    }
    assert!(normalized.contains("#EXT-X-PLAYLIST-TYPE:VOD\n"));
    assert!(normalized.ends_with("#EXT-X-ENDLIST\n"));
}

#[test]
fn real_ffmpeg_master_references_resolve_public_playlist_and_physical_variant() {
    let session = Uuid::now_v7();
    let normalized = normalize_manifest(MASTER, session, "auto").unwrap();
    assert!(normalized.contains(&format!("/api/v1/transcode/{session}/720p/index.m3u8\n")));
    assert!(normalized.contains("BANDWIDTH=373606"));
    assert_eq!(
        rendition_playlist(
            MASTER,
            Path::new("fixture"),
            Path::new("fixture/manifest.m3u8"),
            "auto",
            "720p"
        )
        .unwrap(),
        Path::new("fixture/720p/index.m3u8")
    );
    assert!(
        rendition_playlist(
            MASTER,
            Path::new("fixture"),
            Path::new("fixture/manifest.m3u8"),
            "auto",
            "1080p"
        )
        .is_err()
    );
}

#[test]
fn legacy_flat_variant_paths_remain_relative_to_the_owned_session() {
    let master = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=500000\n720p_index.m3u8\n";
    assert_eq!(
        rendition_playlist(
            master,
            Path::new("fixture"),
            Path::new("fixture/manifest.m3u8"),
            "auto",
            "720p"
        )
        .unwrap(),
        Path::new("fixture/720p_index.m3u8")
    );
    assert!(
        normalize_manifest(master, Uuid::now_v7(), "auto")
            .unwrap()
            .contains("/720p/index.m3u8")
    );
}

#[test]
fn only_the_real_single_rendition_is_served_without_fallback() {
    let root = Path::new("fixture");
    let manifest = Path::new("fixture/manifest.m3u8");
    assert_eq!(
        rendition_playlist(MEDIA, root, manifest, "remux", "remux").unwrap(),
        manifest
    );
    for name in [
        "segments",
        "720p",
        "../remux",
        "%2e%2e",
        "remux/child",
        "remux\\child",
        "",
    ] {
        assert!(rendition_playlist(MEDIA, root, manifest, "remux", name).is_err());
    }
}

#[test]
fn initialization_and_decimal_segments_are_the_only_allowed_files() {
    for name in ["init.mp4", "seg_0000.m4s", "seg_10000.m4s"] {
        assert!(validate_segment_filename(name).is_ok());
    }
    for name in [
        "seg_.m4s",
        "seg_secrets.m4s",
        "seg_0.txt",
        "seg_0.m4s.backup",
        "../init.mp4",
        "seg_../secret.m4s",
        "/init.mp4",
        "init.mp4?token=secret",
        "%2e%2e",
        "init.mp4\\child",
        "manifest.m3u8",
    ] {
        assert!(validate_segment_filename(name).is_err(), "{name}");
    }
}

#[test]
fn output_cannot_redirect_media_requests_or_read_unowned_files() {
    for uri in [
        "../seg_0000.m4s",
        "https://other.invalid/seg_0000.m4s",
        "/seg_0000.m4s",
        "seg_0000.m4s?key=secret",
        "seg_0000.m4s#fragment",
        "seg_0000%2em4s",
    ] {
        assert!(
            normalize_manifest(
                &format!("#EXTM3U\n#EXTINF:4,\n{uri}\n"),
                Uuid::now_v7(),
                "remux"
            )
            .is_err()
        );
    }
    for uri in [
        "../720p/index.m3u8",
        "/720p/index.m3u8",
        "https://other.invalid/720p/index.m3u8",
        "720p/../../index.m3u8",
        "720p\\index.m3u8",
        "720p/index.m3u8?token=secret",
    ] {
        assert!(
            normalize_manifest(
                &format!("#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\n{uri}\n"),
                Uuid::now_v7(),
                "auto"
            )
            .is_err()
        );
    }
    for line in [
        "#EXT-X-MAP:URI=init.mp4",
        "#EXT-X-MAP:URI=\"../init.mp4\"",
        "#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"",
        "#EXT-X-MAP:URI=\"init.mp4\",URI=\"other.mp4\"",
    ] {
        assert!(
            normalize_manifest(&format!("#EXTM3U\n{line}\n"), Uuid::now_v7(), "remux").is_err()
        );
    }
}

#[test]
fn master_generation_preserves_bits_per_second_and_relative_variant_paths() {
    let rendition = TranscodeRendition {
        name: "720p".into(),
        width: 1280,
        height: 720,
        video_bitrate: 3_000_000,
        audio_bitrate: 192_000,
        audio_channels: 2,
    };
    let session = Uuid::now_v7();
    let raw = generate_master_manifest(session, &[rendition]);
    assert!(raw.contains("BANDWIDTH=3192000"));
    assert!(raw.contains("\n720p/index.m3u8\n"));
    assert!(
        normalize_manifest(&raw, session, "auto")
            .unwrap()
            .contains(&format!("/api/v1/transcode/{session}/720p/index.m3u8"))
    );
}

#[test]
fn unsupported_rendition_resources_and_ambiguous_variants_are_rejected() {
    let stream = "#EXT-X-STREAM-INF:BANDWIDTH=1";
    for body in [
        format!("{stream}\n720p/index.m3u8\n{stream}\n720p_index.m3u8\n"),
        format!("#EXT-X-MEDIA:TYPE=AUDIO,URI=\"audio/index.m3u8\"\n{stream}\n720p/index.m3u8\n"),
        format!(
            "#EXT-X-I-FRAME-STREAM-INF:BANDWIDTH=1,URI=\"720p/index.m3u8\"\n{stream}\n720p/index.m3u8\n"
        ),
    ] {
        assert!(normalize_manifest(&format!("#EXTM3U\n{body}"), Uuid::now_v7(), "auto").is_err());
    }
}
