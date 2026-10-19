// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::domains::playback::direct::ResolvedTracks;
use crate::domains::playback::media_source::{MediaFileDetails, build_media_file_info};
use crate::domains::playback::{direct, service};
use crate::services::source_capabilities::{ScanIdentity, VideoDescriptor};
use crate::state::RuntimeConfig;
use uuid::Uuid;

struct Fixture {
    source: ResolvedSource,
    request: StartPlaybackRequest,
    device: DeviceCapabilities,
    network: NetworkConditions,
    config: DecisionEngineConfig,
    descriptor: PlaybackDescriptor,
    fallback: PreparedFallback,
    actor: PlaybackActor,
}

impl Fixture {
    fn new() -> Self {
        let file = Uuid::now_v7();
        let item = Uuid::now_v7();
        let updated = chrono::Utc::now();
        let details = MediaFileDetails {
            id: file,
            file_size: 16,
            updated_at: updated,
            file_modified_at: Some(updated),
            file_path: "source.mp4".into(),
            container_format: "mp4".into(),
            video_codec: Some("hevc".into()),
            video_resolution: Some("320x180".into()),
            video_bitrate: Some(80_000),
            video_dynamic_range: Some("sdr".into()),
            video_frame_rate: Some(24.0),
            audio_codec: Some("opus".into()),
            audio_channels: Some(1),
            audio_language: None,
            audio_bitrate: Some(64_000),
            runtime_seconds: 8,
            additional_streams: json!({"format":{"duration_seconds":8.0},"video":{"index":0,"bit_depth":8,"profile":"Main","level":4.0,"frame_rate":24.0,
                "color_primaries":"bt709","color_transfer":"bt709","color_space":"bt709","color_range":"tv"},
                "audio":[{"index":2,"codec":"opus","channels":1,"bitrate":64000,"sample_rate":48000,"disposition":{"default":1}}]}),
        };
        let (media, tracks) = build_media_file_info(&details, None, None).unwrap();
        let identity = FileIdentity {
            file_id: file,
            path: details.file_path.clone(),
            bytes: 16,
            updated_at: updated,
            modified_ns: "1".into(),
            probe_hash: "0".repeat(64),
        };
        let fallback = PreparedFallback {
            identity,
            tracks: ResolvedTracks::from_selected(&tracks).unwrap(),
            limits: direct::AdmissionLimits {
                max_resolution: (1920, 1080),
                max_audio_channels: 2,
                max_bitrate: 4_000_000,
                quality_mode: "auto".into(),
                requested_bitrate: None,
            },
            descriptor: DirectFallbackResponse {
                video: DirectFallbackVideo {
                    content_type: direct::VIDEO_CONTENT_TYPE.into(),
                    width: 320,
                    height: 180,
                    bitrate: 80_000,
                    framerate: 30,
                },
                audio: DirectFallbackAudio {
                    content_type: direct::AUDIO_CONTENT_TYPE.into(),
                    channels: "1".into(),
                    bitrate: 64_000,
                    samplerate: 48_000,
                },
            },
        };
        let descriptor = PlaybackDescriptor {
            version: 1,
            scan: ScanIdentity {
                size: 16,
                modified_ns: "1".into(),
            },
            moving_video_count: 1,
            video: VideoDescriptor {
                index: 0,
                codec: "hevc".into(),
                file_codec: Some("hvc1.1.6.L120.90".into()),
                copy_codec: "hev1.1.6.L120.90".into(),
                width: 320,
                height: 180,
                bit_depth: 8,
                frame_rate: 24.0,
                bitrate: Some(80_000),
                bitrate_basis: Some("stream".into()),
                config_sha256: "0".repeat(64),
                dynamic_range: "sdr".into(),
            },
            audio: vec![AudioDescriptor {
                index: 2,
                codec: "opus".into(),
                file_codec: "opus".into(),
                copy_codec: None,
                bitrate: Some(64_000),
                sample_rate: 48_000,
                channels: 1,
                config_sha256: "1".repeat(64),
            }],
        };
        let runtime = RuntimeConfig::default();
        Self {
            source: ResolvedSource {
                media_item_id: item,
                library_id: Uuid::now_v7(),
                playback_mode: "interactive".into(),
                ambient_channel: None,
                details,
                media,
                tracks,
            },
            request: serde_json::from_value(
                json!({"media_item_id":item,"media_file_id":file,"direct_recovery_protocol":1}),
            )
            .unwrap(),
            device: service::build_device_capabilities(None, &runtime),
            config: service::build_decision_engine_config(&runtime, Some("auto"), None),
            network: NetworkConditions {
                estimated_throughput_bps: None,
                network_tier: None,
            },
            descriptor,
            fallback,
            actor: PlaybackActor {
                user_id: Uuid::now_v7(),
                profile_id: Uuid::now_v7(),
                has_all_library_access: true,
            },
        }
    }

    fn context(&self, transport: CredentialTransport) -> Context<'_> {
        Context {
            source: &self.source,
            actor: self.actor,
            request: &self.request,
            device: &self.device,
            network: &self.network,
            config: &self.config,
            transport,
        }
    }

    fn candidates(&self, transport: CredentialTransport) -> Vec<Candidate> {
        build(
            &self.context(transport),
            &self.descriptor,
            &self.descriptor.audio[0],
            Some(&self.fallback),
        )
    }
}

#[test]
fn unqualified_sdr_color_remains_on_the_conservative_route() {
    let mut fixture = Fixture::new();
    assert!(eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.source.details.additional_streams["video"]["color_primaries"] = json!("bt2020");
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.source.details.additional_streams["video"]["color_primaries"] = json!("bt709");
    fixture.source.details.additional_streams["video"]["color_space"] = json!(null);
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
}

#[tokio::test]
async fn verified_descriptor_ceiling_bounds_prepared_fallback_without_changing_library_metadata()
-> anyhow::Result<()> {
    struct Directory(std::path::PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let directory =
        Directory(std::env::temp_dir().join(format!("duskcue-source-budget-{}", Uuid::now_v7())));
    tokio::fs::create_dir(&directory.0).await?;
    let path = directory.0.join("source.mkv");
    tokio::fs::write(&path, b"bounded-reference").await?;
    let metadata = tokio::fs::metadata(&path).await?;
    let mut fixture = Fixture::new();
    fixture.source.details.file_path = path.to_string_lossy().into_owned();
    fixture.source.details.file_size = i64::try_from(metadata.len())?;
    fixture.source.details.file_modified_at = Some(metadata.modified()?.into());
    fixture.source.details.container_format = "mkv".into();
    fixture.source.details.video_bitrate = None;
    fixture.source.details.audio_bitrate = None;
    fixture.source.media.video_bitrate_bps = 0;
    fixture.source.media.audio_bitrate_bps = 0;
    fixture.descriptor.scan = ScanIdentity::from_metadata(&metadata).unwrap();
    fixture.descriptor.video.bitrate = Some(341_199);
    fixture.descriptor.video.bitrate_basis = Some("container_average".into());
    fixture.source.details.additional_streams["playback_descriptor"] = json!(fixture.descriptor);
    let before = fixture.source.details.additional_streams.clone();
    let prepared = prepare(fixture.context(CredentialTransport::Bearer)).await;
    let candidate = prepared
        .fallback
        .expect("verified source fallback should be admitted");
    assert!(candidate.descriptor.video.bitrate <= 341_199);
    assert_eq!(candidate.descriptor.audio.bitrate, 64_000);
    assert_eq!(fixture.source.media.video_bitrate_bps, 0);
    assert_eq!(fixture.source.media.audio_bitrate_bps, 0);
    assert_eq!(fixture.source.details.additional_streams, before);
    fixture.source.media.video_bitrate_bps = 80_000;
    fixture.source.media.audio_bitrate_bps = 48_000;
    let bounded = fallback_media(
        &fixture.source,
        &fixture.descriptor,
        &fixture.descriptor.audio[0],
    );
    assert_eq!(
        (bounded.video_bitrate_bps, bounded.audio_bitrate_bps),
        (80_000, 48_000)
    );
    Ok(())
}

#[test]
fn source_duration_uses_actual_fractional_seconds_in_admission_context_and_revision() {
    let mut fixture = Fixture::new();
    let candidates = fixture.candidates(CredentialTransport::Bearer);
    let preparation = Preparation {
        response: PreparePlaybackResponse {
            protocol: 1,
            source_revision: "a".repeat(64),
            candidates: Vec::new(),
        },
        candidates,
        identity: Some(fixture.fallback.identity.clone()),
        fallback: Some(fixture.fallback.clone()),
    };
    let mut revisions = Vec::new();
    for (seconds, rounded, expected) in [(8.4, 8, 8400), (8.6, 9, 8600), (8.6009, 9, 8600)] {
        fixture.source.details.runtime_seconds = rounded;
        fixture.source.details.additional_streams["format"]["duration_seconds"] = json!(seconds);
        assert!(eligible(
            &fixture.source,
            &fixture.request,
            &fixture.descriptor
        ));
        assert_eq!(duration_ms(&fixture.source), Some(expected));
        assert_eq!(
            preparation
                .accept(&preparation.candidates[0], &fixture.source)
                .unwrap()
                .duration_ms,
            expected
        );
        revisions.push(revision(
            &fixture.context(CredentialTransport::Bearer),
            Some(&fixture.fallback.identity),
            &[],
        ));
    }
    assert_ne!(revisions[0], revisions[1]);
    assert_ne!(revisions[1], revisions[2]);
    for value in [
        json!(null),
        json!("NaN"),
        json!(f64::NAN),
        json!(f64::INFINITY),
        json!(f64::MAX),
        json!(-1),
        json!(0),
        json!(0.0001),
        json!(2_147_483.648),
    ] {
        fixture.source.details.additional_streams["format"]["duration_seconds"] = value;
        assert_eq!(duration_ms(&fixture.source), None);
        assert!(!eligible(
            &fixture.source,
            &fixture.request,
            &fixture.descriptor
        ));
        assert!(
            preparation
                .accept(&preparation.candidates[0], &fixture.source)
                .is_none()
        );
    }
    fixture
        .source
        .details
        .additional_streams
        .as_object_mut()
        .unwrap()
        .remove("format");
    assert_eq!(duration_ms(&fixture.source), None);
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
}

#[test]
fn native_and_mse_candidates_retain_exact_delivery_and_never_copy_opus_to_hls() {
    let fixture = Fixture::new();
    let cookie = fixture.candidates(CredentialTransport::Cookie);
    assert_eq!(
        cookie
            .iter()
            .map(|candidate| candidate.response.id)
            .collect::<Vec<_>>(),
        vec!["native_file", "hls_video_copy_aac"]
    );
    assert!(
        cookie[0]
            .response
            .configuration
            .video
            .content_type
            .contains("hvc1.")
    );
    assert!(
        cookie[0]
            .response
            .configuration
            .audio
            .content_type
            .contains("opus")
    );
    let bearer = fixture.candidates(CredentialTransport::Bearer);
    assert_eq!(bearer.len(), 1);
    assert_eq!(bearer[0].response.id, "hls_video_copy_aac");
    assert!(
        bearer[0]
            .response
            .configuration
            .video
            .content_type
            .contains("hev1.")
    );
    assert_eq!(bearer[0].response.configuration.audio.samplerate, 48_000);
    assert_eq!(bearer[0].plan.unwrap().video, StreamAction::Copy);
    assert_eq!(bearer[0].plan.unwrap().audio, StreamAction::Encode);
}

#[test]
fn exact_positive_support_smoothness_and_delivery_are_required_independently_of_power() {
    let fixture = Fixture::new();
    let candidates = fixture.candidates(CredentialTransport::Bearer);
    let preparation = Preparation {
        response: PreparePlaybackResponse {
            protocol: 1,
            source_revision: "a".repeat(64),
            candidates: candidates
                .iter()
                .map(|candidate| candidate.response.clone())
                .collect(),
        },
        candidates,
        identity: Some(fixture.fallback.identity.clone()),
        fallback: Some(fixture.fallback.clone()),
    };
    let mut report:SourceCapabilitiesRequest = serde_json::from_value(json!({"version":1,"source_revision":"a".repeat(64),
        "observations":[{"candidate_id":"hls_video_copy_aac","status":"reported","supported":true,"smooth":true,"power_efficient":false,"delivery_supported":true}]})).unwrap();
    assert!(preparation.select(Some(&report)).is_some());
    for state in [Some(false), None] {
        report.observations[0].smooth = state;
        assert!(preparation.select(Some(&report)).is_none());
    }
    report.observations[0].smooth = Some(true);
    report.observations[0].delivery_supported = None;
    assert!(preparation.select(Some(&report)).is_none());
    report.observations[0].delivery_supported = Some(true);
    report.observations[0].supported = Some(false);
    assert!(preparation.select(Some(&report)).is_none());
    report.observations[0].supported = Some(true);
    report.observations[0].status = "timed_out".into();
    assert!(preparation.select(Some(&report)).is_none());
    assert!(preparation.select(None).is_none());
}

#[test]
fn stale_unknown_duplicate_and_versioned_reports_cannot_select_a_route() {
    let fixture = Fixture::new();
    let candidates = fixture.candidates(CredentialTransport::Bearer);
    let preparation = Preparation {
        response: PreparePlaybackResponse {
            protocol: 1,
            source_revision: "a".repeat(64),
            candidates: Vec::new(),
        },
        candidates,
        identity: None,
        fallback: None,
    };
    let report = json!({"version":1,"source_revision":"a".repeat(64),"observations":[{"candidate_id":"hls_video_copy_aac","status":"reported","supported":true,"smooth":true,"delivery_supported":true}]});
    for changed in [
        json!({"version":2}),
        json!({"source_revision":"b".repeat(64)}),
        json!({"observations":[report["observations"][0].clone(),report["observations"][0].clone()]}),
        json!({"observations":[{"candidate_id":"unproposed","status":"reported","supported":true,"smooth":true,"delivery_supported":true}]}),
    ] {
        let mut body = report.clone();
        for (key, value) in changed.as_object().unwrap() {
            body[key] = value.clone();
        }
        let request: SourceCapabilitiesRequest = serde_json::from_value(body).unwrap();
        assert!(preparation.select(Some(&request)).is_none());
    }
}

#[test]
fn profile_transport_tracks_quality_and_source_revision_are_bound() {
    let mut fixture = Fixture::new();
    let base = revision(
        &fixture.context(CredentialTransport::Cookie),
        Some(&fixture.fallback.identity),
        &[],
    );
    assert_ne!(
        base,
        revision(
            &fixture.context(CredentialTransport::Bearer),
            Some(&fixture.fallback.identity),
            &[]
        )
    );
    fixture.actor.profile_id = Uuid::now_v7();
    assert_ne!(
        base,
        revision(
            &fixture.context(CredentialTransport::Cookie),
            Some(&fixture.fallback.identity),
            &[]
        )
    );
    fixture.source.tracks.audio_stream_index = Some(1);
    let tracks = revision(
        &fixture.context(CredentialTransport::Cookie),
        Some(&fixture.fallback.identity),
        &[],
    );
    fixture.source.tracks.audio_stream_index = Some(2);
    assert_ne!(
        tracks,
        revision(
            &fixture.context(CredentialTransport::Cookie),
            Some(&fixture.fallback.identity),
            &[]
        )
    );
    fixture.request.max_streaming_bitrate = Some(100_000);
    let quality = revision(
        &fixture.context(CredentialTransport::Cookie),
        Some(&fixture.fallback.identity),
        &[],
    );
    fixture.request.max_streaming_bitrate = None;
    assert_ne!(
        quality,
        revision(
            &fixture.context(CredentialTransport::Cookie),
            Some(&fixture.fallback.identity),
            &[]
        )
    );
    let mut identity = fixture.fallback.identity.clone();
    identity.modified_ns = "2".into();
    assert_ne!(
        revision(
            &fixture.context(CredentialTransport::Cookie),
            Some(&identity),
            &[]
        ),
        revision(
            &fixture.context(CredentialTransport::Cookie),
            Some(&fixture.fallback.identity),
            &[]
        )
    );
}

#[test]
fn manual_hdr_missing_bitrate_burn_in_and_ambiguous_native_tracks_are_not_promoted() {
    let mut fixture = Fixture::new();
    assert!(eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.request.quality_mode = Some("manual".into());
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.request.quality_mode = None;
    fixture.descriptor.video.dynamic_range = "hdr10".into();
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.descriptor.video.dynamic_range = "sdr".into();
    fixture.descriptor.video.bitrate = None;
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.descriptor.video.bitrate = Some(80_000);
    fixture.source.tracks.subtitle_stream_index = Some(3);
    assert!(!eligible(
        &fixture.source,
        &fixture.request,
        &fixture.descriptor
    ));
    fixture.source.tracks.subtitle_stream_index = None;
    fixture.request.audio_stream_index = Some(2);
    assert!(
        fixture
            .candidates(CredentialTransport::Cookie)
            .iter()
            .all(|candidate| candidate.response.delivery == "hls")
    );
    fixture.request.audio_stream_index = None;
    fixture.source.details.additional_streams["audio"]
        .as_array_mut()
        .unwrap()
        .push(json!({"index":1,"disposition":{"default":1}}));
    assert!(!native_default(&fixture.source));
}

#[test]
fn hls_source_audio_copy_is_offered_only_for_validated_aac() {
    let mut fixture = Fixture::new();
    fixture.source.media.audio_codec = "aac".into();
    fixture.descriptor.audio[0].codec = "aac".into();
    fixture.descriptor.audio[0].file_codec = "mp4a.40.2".into();
    fixture.descriptor.audio[0].copy_codec = Some("mp4a.40.2".into());
    let candidates = fixture.candidates(CredentialTransport::Bearer);
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].response.id, "hls_video_copy_audio_copy");
    assert_eq!(candidates[0].plan.unwrap().audio, StreamAction::Copy);
    fixture.device.max_audio_channels = 0;
    assert!(
        fixture
            .candidates(CredentialTransport::Bearer)
            .iter()
            .all(|candidate| candidate.response.id != "hls_video_copy_audio_copy")
    );
}

#[test]
fn unknown_source_audio_bitrate_does_not_hide_an_exact_aac_encoded_output() {
    let mut fixture = Fixture::new();
    fixture.descriptor.audio[0].bitrate = None;
    let candidates = fixture.candidates(CredentialTransport::Cookie);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].response.id, "hls_video_copy_aac");
    assert_eq!(candidates[0].response.configuration.audio.bitrate, 64_000);
    assert_eq!(
        candidates[0].response.configuration.audio.samplerate,
        48_000
    );
}
