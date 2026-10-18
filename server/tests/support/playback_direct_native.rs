// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AppState, ProgressiveFixture, direct_media, process};

pub(super) struct Candidate {
    pub item: Uuid,
    pub file: Uuid,
    pub path: PathBuf,
    pub sha256: String,
}

impl Candidate {
    pub(super) fn ready(&self) -> Value {
        json!({"item":self.item,"file":self.file,"title":"Native worker recovery fixture","audioHz":880,"runtimeSeconds":60,"sha256":self.sha256})
    }
}

pub(super) fn enabled() -> bool {
    std::env::var("DUSKCUE_TEST_NATIVE_RECOVERY").as_deref() == Ok("live")
}

pub(super) async fn prepare(
    tools: &process::FixtureProcesses,
    source: &Path,
    root: &Path,
) -> anyhow::Result<Candidate> {
    anyhow::ensure!(
        cfg!(feature = "playback-qualification"),
        "native recovery requires its qualification feature"
    );
    anyhow::ensure!(
        std::env::var("DUSKCUE_TEST_NATIVE_CLIENT").as_deref() == Ok("live")
            && std::env::var("DUSKCUE_TEST_DIRECT_BROWSER").as_deref() == Ok("live")
    );
    let path = root.join("native-recovery.mp4");
    process::original(
        tools,
        "ffmpeg",
        &[
            "-hide_banner".into(),
            "-loglevel".into(),
            "error".into(),
            "-nostdin".into(),
            "-stream_loop".into(),
            "-1".into(),
            "-i".into(),
            source.to_string_lossy().into_owned(),
            "-map".into(),
            "0".into(),
            "-c".into(),
            "copy".into(),
            "-t".into(),
            "60".into(),
            "-fs".into(),
            "16777216".into(),
            "-movflags".into(),
            "+faststart".into(),
            "-n".into(),
            path.to_string_lossy().into_owned(),
        ],
    )
    .await?;
    let sha256 = direct_media::file_hash(&path).await?;
    Ok(Candidate {
        item: Uuid::now_v7(),
        file: Uuid::now_v7(),
        path,
        sha256,
    })
}

pub(super) async fn configure(
    state: &mut AppState,
    root: &Path,
    candidate: &Candidate,
) -> anyhow::Result<()> {
    #[cfg(feature = "playback-qualification")]
    {
        use duskcue::services::transcoding::{TranscodeManager, qualification::OwnedPacedInput};
        let input = OwnedPacedInput::new(root, &candidate.path).await?;
        anyhow::ensure!(state.transcode_manager.active_session_count() == 0);
        state.transcode_manager = std::sync::Arc::new(
            TranscodeManager::new(state.runtime_config.clone()).with_qualification_input(input),
        );
        Ok(())
    }
    #[cfg(not(feature = "playback-qualification"))]
    {
        let _ = (state, root, candidate);
        anyhow::bail!("native recovery requires its qualification feature")
    }
}

pub(super) async fn cleanup(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    candidate: &Candidate,
) -> anyhow::Result<()> {
    let unchanged = direct_media::file_hash(&candidate.path).await? == candidate.sha256;
    let plays: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM play_sessions WHERE media_item_id=$1 AND user_id=$2 AND stopped_at IS NULL",
    )
    .bind(candidate.item)
    .bind(fixture.user)
    .fetch_all(fixture.pool)
    .await?;
    let mut failures = Vec::new();
    for play in plays {
        let stopped = tokio::time::timeout(
            Duration::from_secs(10),
            duskcue::domains::playback::service::stop_playback(
                fixture.pool,
                &state.transcode_manager,
                fixture.user,
                play,
                None,
                false,
            ),
        )
        .await;
        if !matches!(stopped, Ok(Ok(_))) {
            failures.push("native playback stop failed");
        }
    }
    let active = state
        .transcode_manager
        .list_active_sessions()
        .iter()
        .any(|session| session.media_file_id == candidate.file);
    if active {
        failures.push("native manager retains a worker");
    }
    if !unchanged {
        failures.push("native fixture source changed");
    }
    if failures.is_empty() {
        sqlx::query("DELETE FROM media_items WHERE id=$1 AND library_id=$2")
            .bind(candidate.item)
            .bind(fixture.library)
            .execute(fixture.pool)
            .await?;
    }
    anyhow::ensure!(
        failures.is_empty(),
        "native fixture cleanup failed: {failures:?}"
    );
    Ok(())
}
