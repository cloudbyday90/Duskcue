// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use uuid::Uuid;

use super::{INPUT_ARGS, MAX_INPUT_BYTES, OwnedPacedInput};
use crate::domains::playback::PlaybackError;
use crate::services::transcoding::arguments::build_ffmpeg_input_args;
use crate::services::transcoding::plan::{StreamAction, StreamPlan};
use crate::services::transcoding::{
    HwAccelMethod, OutputProfile, StartSessionParams, TranscodeManager, TranscodeSession,
};

struct Directory {
    parent: PathBuf,
    root: PathBuf,
    source: PathBuf,
}

impl Directory {
    async fn new() -> anyhow::Result<Self> {
        let parent = tokio::fs::canonicalize(std::env::temp_dir()).await?;
        Self::create(
            parent.join(format!("duskcue-paced-input-{}", Uuid::now_v7())),
            parent,
        )
        .await
    }

    async fn create(root: PathBuf, parent: PathBuf) -> anyhow::Result<Self> {
        anyhow::ensure!(root.parent() == Some(parent.as_path()));
        tokio::fs::create_dir(&root).await?;
        let fixture = Self {
            source: root.join("source.mp4"),
            root,
            parent,
        };
        tokio::fs::write(&fixture.source, b"owned input").await?;
        Ok(fixture)
    }

    async fn input(&self) -> anyhow::Result<OwnedPacedInput> {
        Ok(OwnedPacedInput::new(&self.root, &self.source).await?)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        if self.root.parent() == Some(self.parent.as_path())
            && self
                .root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("duskcue-paced-input-"))
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn manager() -> TranscodeManager {
    let mut config = crate::state::RuntimeConfig::default();
    config.cpu.hw_accel_auto_detect = false;
    config.resource_limits.max_concurrent_transcodes = 1;
    TranscodeManager::new(Arc::new(arc_swap::ArcSwap::from_pointee(config)))
}

fn session(source: &Path, profile: Option<OutputProfile>) -> TranscodeSession {
    let encode = profile.is_some();
    TranscodeSession {
        id: Uuid::now_v7(),
        media_file_id: Uuid::now_v7(),
        user_id: Uuid::now_v7(),
        started_at: chrono::Utc::now(),
        source_path: source.to_owned(),
        source_video_codec: "h264".into(),
        source_video_resolution: (320, 180),
        source_audio_codec: "aac".into(),
        source_video_stream_index: Some(1),
        source_audio_stream_index: Some(2),
        stream_plan: StreamPlan {
            video: if encode {
                StreamAction::Encode
            } else {
                StreamAction::Copy
            },
            audio: if encode {
                StreamAction::Encode
            } else {
                StreamAction::Copy
            },
        },
        output_profile: profile,
        target_audio_channels: encode.then_some(1),
        target_audio_bitrate: encode.then_some(64_000),
        subtitle_stream_index: None,
        subtitle_stream_ordinal: None,
        subtitle_burn_in: false,
        target_video_codec: "h264".into(),
        target_video_resolution: (320, 180),
        target_audio_codec: "aac".into(),
        target_bitrate: if encode { 400_000 } else { 0 },
        hw_accel: HwAccelMethod::Software,
        source_start_ms: 0,
        recovery_attempts: 1,
        rendition_name: "source".into(),
        segment_dir: source.parent().unwrap().join("not-created"),
        manifest_path: source.parent().unwrap().join("not-created/manifest.m3u8"),
        segments_written: 0,
        client_position_segment: 0,
        progress: None,
        is_complete: false,
        is_seeking: false,
    }
}

#[tokio::test]
async fn ordinary_input_assembly_is_unchanged_without_qualification_policy() -> anyhow::Result<()> {
    let manager = manager();
    let source = Path::new("ordinary-nonexistent.mp4");
    for position in [None, Some(-1), Some(0), Some(4000), Some(6123)] {
        assert_eq!(
            manager.build_session_input_args(position, source).await?,
            build_ffmpeg_input_args(position, source)
        );
    }
    assert_eq!(manager.active_session_count(), 0);
    Ok(())
}

#[tokio::test]
async fn pacing_is_exact_source_scoped_and_does_not_leak_between_managers() -> anyhow::Result<()> {
    let fixture = Directory::new().await?;
    let other = fixture.root.join("another.mp4");
    tokio::fs::write(&other, b"another input").await?;
    let paced = manager().with_qualification_input(fixture.input().await?);
    let ordinary = manager();
    let (selected, unselected, independent) = tokio::join!(
        paced.build_session_input_args(Some(4000), &fixture.source),
        paced.build_session_input_args(Some(4000), &other),
        ordinary.build_session_input_args(Some(4000), &fixture.source)
    );
    let selected = selected?;
    assert_eq!(selected[..INPUT_ARGS.len()], INPUT_ARGS);
    assert_eq!(
        selected[INPUT_ARGS.len()..],
        build_ffmpeg_input_args(Some(4000), &fixture.source)
    );
    let input = selected
        .iter()
        .position(|argument| argument == "-i")
        .unwrap();
    for flag in [
        "-readrate",
        "-readrate_initial_burst",
        "-readrate_catchup",
        "-ss",
    ] {
        let positions = selected
            .iter()
            .enumerate()
            .filter_map(|(index, value)| (value == flag).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(positions.len(), 1);
        assert!(positions[0] < input);
    }
    assert_eq!(unselected?, build_ffmpeg_input_args(Some(4000), &other));
    assert_eq!(
        independent?,
        build_ffmpeg_input_args(Some(4000), &fixture.source)
    );
    Ok(())
}

#[tokio::test]
async fn owned_input_rejects_aliases_and_non_owned_or_non_regular_paths() -> anyhow::Result<()> {
    let fixture = Directory::new().await?;
    let sibling = Directory::create(
        fixture.root.with_extension("sibling"),
        fixture.parent.clone(),
    )
    .await?;
    let alias = fixture.root.join("alias.mp4");
    std::os::unix::fs::symlink(&fixture.source, &alias)?;
    let missing = fixture.root.join("missing.mp4");
    let noncanonical = fixture
        .root
        .join("..")
        .join(fixture.root.file_name().unwrap())
        .join("source.mp4");
    for source in [
        sibling.source.as_path(),
        alias.as_path(),
        fixture.root.as_path(),
        missing.as_path(),
        noncanonical.as_path(),
    ] {
        assert!(OwnedPacedInput::new(&fixture.root, source).await.is_err());
    }
    assert!(
        OwnedPacedInput::new(&fixture.source, &fixture.source)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn owned_input_enforces_nonempty_sixteen_mib_limit() -> anyhow::Result<()> {
    let fixture = Directory::new().await?;
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(&fixture.source)
        .await?;
    file.set_len(0).await?;
    assert!(fixture.input().await.is_err());
    file.set_len(MAX_INPUT_BYTES).await?;
    let admitted = fixture.input().await?;
    assert!(admitted.applies(&fixture.source).await?);
    file.set_len(MAX_INPUT_BYTES + 1).await?;
    assert!(fixture.input().await.is_err());
    assert!(admitted.applies(&fixture.source).await.is_err());
    Ok(())
}

#[tokio::test]
async fn replaced_or_redirected_configured_source_refuses_input_assembly() -> anyhow::Result<()> {
    for redirect in [false, true] {
        let fixture = Directory::new().await?;
        let paced = manager().with_qualification_input(fixture.input().await?);
        let replacement = fixture.root.join("replacement.mp4");
        tokio::fs::write(&replacement, b"owned input").await?;
        if redirect {
            tokio::fs::remove_file(&fixture.source).await?;
            std::os::unix::fs::symlink(&replacement, &fixture.source)?;
        } else {
            tokio::fs::rename(&replacement, &fixture.source).await?;
        }
        assert!(
            paced
                .build_session_input_args(None, &fixture.source)
                .await
                .is_err()
        );
        assert_eq!(paced.active_session_count(), 0);
    }
    Ok(())
}

#[tokio::test]
async fn changed_source_refuses_real_start_before_cache_or_worker_allocation() -> anyhow::Result<()>
{
    let fixture = Directory::new().await?;
    let paced = manager().with_qualification_input(fixture.input().await?);
    tokio::fs::write(&fixture.source, b"changed owned input").await?;
    let params: StartSessionParams = session(&fixture.source, None).restart_params(4000);
    let data = fixture.root.join("unallocated-data");
    let result = paced.start_session(params, &data).await;
    assert!(matches!(result, Err(PlaybackError::FfmpegFailed(_))));
    assert!(!data.exists());
    assert_eq!(paced.active_session_count(), 0);
    assert_eq!(paced.semaphore.available_permits(), 1);
    Ok(())
}

#[tokio::test]
async fn same_length_in_place_edit_invalidates_the_admitted_identity() -> anyhow::Result<()> {
    let fixture = Directory::new().await?;
    let mut admitted = fixture.input().await?;
    tokio::fs::write(&fixture.source, b"other input").await?;
    assert_eq!(tokio::fs::metadata(&fixture.source).await?.len(), 11);
    let (retained_metadata, changed_sha256) =
        super::inspect(&fixture.root, &fixture.source).await?;
    assert_eq!(retained_metadata.bytes, admitted.identity.bytes);
    assert_eq!(retained_metadata.device, admitted.identity.device);
    assert_eq!(retained_metadata.inode, admitted.identity.inode);
    assert_ne!(changed_sha256, admitted.sha256);
    admitted.identity = retained_metadata;
    let paced = manager().with_qualification_input(admitted);
    assert!(
        paced
            .build_session_input_args(Some(6000), &fixture.source)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn restarts_reselect_pacing_without_changing_tracks_profile_or_attempts() -> anyhow::Result<()>
{
    let fixture = Directory::new().await?;
    let paced = manager().with_qualification_input(fixture.input().await?);
    for profile in [None, Some(OutputProfile::CompatibleSdr)] {
        let original = session(&fixture.source, profile);
        for position in [4000, 6000] {
            let restart = original.clone().restart_params(position);
            assert_eq!(restart.source_video_stream_index, Some(1));
            assert_eq!(restart.source_audio_stream_index, Some(2));
            assert_eq!(restart.output_profile, profile);
            assert_eq!(restart.stream_plan, original.stream_plan);
            assert_eq!(restart.recovery_attempts, 1);
            assert_eq!(restart.hw_accel_override, Some(HwAccelMethod::Software));
            assert_eq!(restart.target_resolution, Some((320, 180)));
            assert_eq!(restart.target_audio_bitrate, original.target_audio_bitrate);
            let args = paced
                .build_session_input_args(restart.seek_position_ms, &restart.source_path)
                .await?;
            assert_eq!(args[..INPUT_ARGS.len()], INPUT_ARGS);
            assert_eq!(
                args[INPUT_ARGS.len()..],
                build_ffmpeg_input_args(Some(position), &fixture.source)
            );
        }
    }
    Ok(())
}
