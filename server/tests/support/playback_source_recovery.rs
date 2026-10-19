// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use duskcue::services::source_capabilities::PlaybackDescriptor;
use duskcue::services::transcoding::{HwAccelMethod, OutputProfile, plan::StreamAction};
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::policy;
use super::{AppState, ProgressiveFixture, inventory, process, source_http as http, source_media};

#[path = "playback_source_corruption.rs"]
mod corruption;
#[path = "playback_source_recovery_verify.rs"]
mod verify;

struct Candidate {
    item: Uuid,
    file: Uuid,
    path: PathBuf,
    variant: String,
    sha256: String,
}

struct Armed {
    play: Uuid,
    generation: Uuid,
    candidate: Uuid,
    directory: PathBuf,
    route: Value,
}

pub(super) struct Inventory {
    library: Uuid,
    root: PathBuf,
    candidates: Vec<Candidate>,
    completed: BTreeMap<String, Value>,
    sessions: Vec<(Uuid, Uuid, PathBuf)>,
    prepared: bool,
    armed: BTreeMap<String, Armed>,
    authored_damage: Option<Value>,
}

impl Inventory {
    pub(super) fn new(root: &Path) -> Self {
        Self {
            library: Uuid::now_v7(),
            root: root.join("source-recovery-sources"),
            candidates: Vec::new(),
            completed: BTreeMap::new(),
            sessions: Vec::new(),
            prepared: false,
            armed: BTreeMap::new(),
            authored_damage: None,
        }
    }

    pub(super) async fn prepare(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        source: &Path,
        root: &Path,
        tools: &process::FixtureProcesses,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(super::source_enabled());
        tokio::fs::create_dir(&self.root).await?;
        self.prepared = true;
        sqlx::query("INSERT INTO libraries(id,name,slug,media_type,root_path) VALUES($1,'Source recovery',$2,'movies',$3)").bind(self.library).bind(format!("source-recovery-{}",self.library)).bind(self.root.to_string_lossy().as_ref()).execute(fixture.pool).await?;
        sqlx::query("INSERT INTO library_paths(library_id,path,is_default) VALUES($1,$2,true)")
            .bind(self.library)
            .bind(self.root.to_string_lossy().as_ref())
            .execute(fixture.pool)
            .await?;
        for (variant, path) in [
            ("clean", source.to_path_buf()),
            ("video-slice", source.to_path_buf()),
            (
                "video-slice-native",
                self.root.join("source-video-slice.mp4"),
            ),
            ("video-empty", root.join("direct-browser-video-empty.mp4")),
        ] {
            let metadata = tokio::fs::symlink_metadata(&path).await?;
            anyhow::ensure!(
                metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() > 0
                    && metadata.len() <= 8 * 1024 * 1024
            );
            let destination = self.root.join(format!("source-{variant}.mp4"));
            if variant == "video-slice" {
                self.authored_damage = Some(corruption::author(tools, source, &destination).await?);
            } else {
                tokio::fs::copy(&path, &destination).await?;
            }
            tokio::fs::set_permissions(&destination, std::fs::Permissions::from_mode(0o444))
                .await?;
            if variant != "video-slice" {
                anyhow::ensure!(
                    inventory::hash(&destination).await? == inventory::hash(&path).await?
                );
            }
        }
        let scan =
            duskcue::workers::library_scanner::scan_library(fixture.pool, self.library, true, None)
                .await?;
        anyhow::ensure!(
            scan.errors.is_empty() && scan.files_new == 4 && scan.items_created == 4,
            "actual source-recovery scanner did not ingest all four owned references"
        );
        for variant in ["clean", "video-slice", "video-slice-native", "video-empty"] {
            let path = self.root.join(format!("source-{variant}.mp4"));
            let (item,file,metadata):(Uuid,Uuid,Value)=sqlx::query_as("SELECT f.media_item_id,f.id,f.additional_streams FROM media_files f JOIN media_items i ON i.id=f.media_item_id WHERE i.library_id=$1 AND f.file_path=$2 AND f.is_healthy=true").bind(self.library).bind(path.to_string_lossy().as_ref()).fetch_one(fixture.pool).await?;
            let descriptor = PlaybackDescriptor::from_metadata(&metadata).ok_or_else(|| {
                anyhow::anyhow!(
                    "source recovery reference lacks an actual scanner descriptor: {variant}"
                )
            })?;
            anyhow::ensure!(
                descriptor.video.index == 1
                    && descriptor.video.codec == "h264"
                    && descriptor.video.bit_depth == 8
                    && descriptor.video.dynamic_range == "sdr"
                    && descriptor.video.copy_codec == "avc1.640028"
                    && descriptor.audio.iter().any(|audio| audio.index == 2
                        && audio.copy_codec.as_deref() == Some("mp4a.40.2"))
            );
            sqlx::query("UPDATE media_items SET title=$2,sort_title=$2,content_rating='R',runtime_seconds=8 WHERE id=$1").bind(item).bind(format!("Source recovery {variant}")).execute(fixture.pool).await?;
            self.candidates.push(Candidate {
                item,
                file,
                sha256: inventory::hash(&path).await?,
                path,
                variant: variant.into(),
            });
        }
        anyhow::ensure!(
            self.candidates
                .iter()
                .map(|value| value.item)
                .collect::<BTreeSet<_>>()
                .len()
                == 4
                && self
                    .candidates
                    .iter()
                    .map(|value| value.file)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 4,
            "actual scanner did not preserve separate source-recovery items and files"
        );
        let browser = self
            .candidates
            .iter()
            .find(|value| value.variant == "video-slice")
            .ok_or_else(|| anyhow::anyhow!("browser source candidate unavailable"))?;
        let native = self
            .candidates
            .iter()
            .find(|value| value.variant == "video-slice-native")
            .ok_or_else(|| anyhow::anyhow!("native source candidate unavailable"))?;
        anyhow::ensure!(
            browser.sha256 == native.sha256
                && browser.path != native.path
                && browser.item != native.item
                && browser.file != native.file
        );
        let watch_rows:i64=sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE user_id=$1 AND profile_id=$2 AND media_item_id=ANY($3)")
            .bind(fixture.user).bind(fixture.original_profile).bind(vec![browser.item,native.item]).fetch_one(fixture.pool).await?;
        anyhow::ensure!(
            watch_rows == 0,
            "new source recovery items unexpectedly inherited watch history"
        );
        let proof = self
            .authored_damage
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("source corruption authoring proof unavailable"))?;
        proof["client_items_isolated"] = json!(true);
        proof["native_bytes_equal_browser"] = json!(true);
        proof["initial_watch_rows"] = json!(watch_rows);
        Ok(())
    }

    pub(super) fn ready(&self) -> Value {
        json!(self.candidates.iter().map(|candidate|json!({"item":candidate.item,"file":candidate.file,"title":format!("Source recovery {}",candidate.variant),"variant":candidate.variant,"sha256":candidate.sha256,"audioHz":880,"videoIndex":1,"audioIndex":2})).collect::<Vec<_>>())
    }

    pub(super) async fn poll(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        root: &Path,
        tools: &process::FixtureProcesses,
        resource: Uuid,
        peer: &str,
        kids: &str,
    ) -> anyhow::Result<()> {
        for client in ["browser", "native"] {
            let arm_phase = format!("source-recovery-armed-{client}");
            if !self.armed.contains_key(client) {
                if let Some(observed) =
                    super::super::read_record(&format!("/tmp/direct-observed-{arm_phase}.json"))
                        .await?
                {
                    anyhow::ensure!(
                        observed["resourceId"] == resource.to_string()
                            && observed["phase"] == arm_phase
                    );
                    self.closed(fixture, state).await?;
                    let checked = verify::arm(self, fixture, state, client, &observed).await;
                    let response = match &checked {
                        Ok((_, backend)) => {
                            json!({"resourceId":resource,"phase":arm_phase,"passed":true,"backend":backend})
                        }
                        Err(_) => {
                            json!({"resourceId":resource,"phase":arm_phase,"passed":false,"error":"Owned source-copy arming verification failed."})
                        }
                    };
                    super::super::write_record(
                        &format!("/tmp/direct-verified-{arm_phase}"),
                        &response,
                    )
                    .await?;
                    let (armed, _) = checked?;
                    self.armed.insert(client.into(), armed);
                }
            }
            let phase = format!("source-recovery-{client}");
            if self.completed.contains_key(&phase) {
                continue;
            }
            let Some(observed) =
                super::super::read_record(&format!("/tmp/direct-observed-{phase}.json")).await?
            else {
                continue;
            };
            anyhow::ensure!(
                observed["resourceId"] == resource.to_string() && observed["phase"] == phase
            );
            self.closed(fixture, state).await?;
            let checked = verify::run(
                self, fixture, state, root, tools, client, &observed, peer, kids,
            )
            .await;
            let response = match &checked {
                Ok(backend) => {
                    json!({"resourceId":resource,"phase":phase,"passed":true,"backend":backend})
                }
                Err(_) => {
                    json!({"resourceId":resource,"phase":phase,"passed":false,"error":"Owned source recovery verification failed."})
                }
            };
            super::super::write_record(&format!("/tmp/direct-verified-{phase}"), &response).await?;
            self.completed.insert(phase, checked?);
        }
        Ok(())
    }

    async fn closed(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<()> {
        for (play, generation, path) in &self.sessions {
            let stopped: bool =
                sqlx::query_scalar("SELECT stopped_at IS NOT NULL FROM play_sessions WHERE id=$1")
                    .bind(play)
                    .fetch_one(fixture.pool)
                    .await?;
            anyhow::ensure!(
                stopped
                    && state.transcode_manager.get_session(generation).is_none()
                    && !path.exists(),
                "source-recovery Close did not retire its logical session and physical generation"
            );
        }
        Ok(())
    }

    pub(super) async fn finish(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(
            self.prepared
                && self.candidates.len() == 4
                && self.completed.len() == 2
                && self.armed.len() == 2,
            "source-recovery client matrix is incomplete"
        );
        self.closed(fixture, state).await?;
        for candidate in &self.candidates {
            anyhow::ensure!(inventory::hash(&candidate.path).await? == candidate.sha256);
        }
        Ok(
            json!({"executed":true,"clients":2,"scanner_ingested":true,"authored_damage":self.authored_damage,"genuine_decoder_failure":true,"sources_unchanged":true,"cleanup_passed":true,"results":self.completed}),
        )
    }

    pub(super) async fn cleanup(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<()> {
        if !self.prepared {
            return Ok(());
        }
        let plays: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM play_sessions WHERE library_id=$1 AND stopped_at IS NULL",
        )
        .bind(self.library)
        .fetch_all(fixture.pool)
        .await?;
        let mut errors = Vec::new();
        for play in plays {
            if !matches!(
                tokio::time::timeout(
                    std::time::Duration::from_secs(10),
                    duskcue::domains::playback::service::stop_playback(
                        fixture.pool,
                        &state.transcode_manager,
                        fixture.user,
                        play,
                        None,
                        false
                    )
                )
                .await,
                Ok(Ok(_))
            ) {
                errors.push("source-recovery stop failed");
            }
        }
        if state
            .transcode_manager
            .list_active_sessions()
            .iter()
            .any(|session| session.source_path.starts_with(&self.root))
        {
            errors.push("source-recovery worker remains");
        }
        sqlx::query("DELETE FROM libraries WHERE id=$1")
            .bind(self.library)
            .execute(fixture.pool)
            .await?;
        anyhow::ensure!(
            errors.is_empty(),
            "source-recovery cleanup failed: {}",
            errors.join("; ")
        );
        Ok(())
    }
}
