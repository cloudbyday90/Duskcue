// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use uuid::Uuid;

use super::{AppState, ProgressiveFixture, process};

#[path = "playback_format_inventory.rs"]
mod inventory;
#[path = "playback_format_media.rs"]
mod media;
#[path = "playback_source_copy_boundary.rs"]
mod source_copy_boundary;
#[path = "playback_source_http.rs"]
mod source_http;
#[path = "playback_source_media.rs"]
mod source_media;
#[path = "playback_source_recovery.rs"]
mod source_recovery;
#[path = "playback_source_scan.rs"]
mod source_scan;
#[path = "playback_source_verify.rs"]
mod source_verify;
#[path = "playback_format_verify.rs"]
mod verify;

pub(super) struct Case {
    index: usize,
    source: Value,
    path: PathBuf,
    item: Uuid,
    file: Uuid,
}

impl Case {
    fn hdr(&self) -> bool {
        self.source["dynamicRange"] != "sdr"
    }
    fn phase(&self, client: &str) -> String {
        format!("format-{client}-{:02}", self.index)
    }
}

pub(super) struct Inventory {
    library: Uuid,
    root: PathBuf,
    cases: Vec<Case>,
    completed: BTreeMap<String, Value>,
    sessions: Vec<(Uuid, Uuid, PathBuf)>,
    source_sessions: Vec<Uuid>,
    source_descriptors: Vec<Value>,
    quick_scan: Option<Value>,
    source_copy_boundary: Option<Value>,
    source_recovery: source_recovery::Inventory,
    peer: Uuid,
    peer_token: String,
    kids_session: Uuid,
    kids_token: String,
}

pub(super) fn enabled() -> bool {
    std::env::var("DUSKCUE_TEST_FORMAT_CORPUS").as_deref() == Ok("live")
}

pub(super) fn source_enabled() -> bool {
    std::env::var("DUSKCUE_TEST_SOURCE_CAPABILITIES").as_deref() == Ok("live")
}

pub(super) fn hdr_refusal() -> (u16, &'static str) {
    (422, "PLAY_020")
}

impl Inventory {
    pub(super) fn new(root: &Path) -> Self {
        Self {
            library: Uuid::now_v7(),
            root: root.join("format-sources"),
            cases: Vec::new(),
            completed: BTreeMap::new(),
            sessions: Vec::new(),
            source_sessions: Vec::new(),
            source_descriptors: Vec::new(),
            quick_scan: None,
            source_copy_boundary: None,
            source_recovery: source_recovery::Inventory::new(root),
            peer: Uuid::now_v7(),
            peer_token: duskcue::domains::auth::service::generate_session_token(),
            kids_session: Uuid::now_v7(),
            kids_token: duskcue::domains::auth::service::generate_session_token(),
        }
    }

    pub(super) fn ready(&self) -> Value {
        Value::Array(self.cases.iter().map(|case| {
            let mut value = json!({"index":case.index,"id":case.source["id"],"master":case.source["master"],"container":case.source["container"],"item":case.item,"file":case.file,"title":format!("Format {}",case.source["id"].as_str().unwrap_or("")),"codec":case.source["codec"],"bitDepth":case.source["bitDepth"],"dynamicRange":case.source["dynamicRange"],"sha256":case.source["sha256"],"videoIndex":0,"audioIndex":2,"audioHz":880,"capability":case.source["capability"]});
            if case.hdr() { let (status, code) = hdr_refusal(); value["expectedStatus"] = json!(status); value["expectedCode"] = json!(code); }
            value
        }).collect())
    }

    pub(super) async fn prepare_sources(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        source: &Path,
        root: &Path,
        tools: &process::FixtureProcesses,
    ) -> anyhow::Result<()> {
        if source_enabled() {
            self.quick_scan = Some(source_scan::run(self, fixture).await?);
            for case in &self.cases {
                let metadata:Value=sqlx::query_scalar("SELECT additional_streams FROM media_files WHERE id=$1 AND media_item_id=$2 AND is_healthy=true").bind(case.file).bind(case.item).fetch_one(fixture.pool).await?;
                self.source_descriptors.push(
                    source_verify::scanner_metadata(&metadata, &case.path, &case.source).await?,
                );
            }
            anyhow::ensure!(self.source_descriptors.len() == 14);
            let reference = self
                .cases
                .iter()
                .find(|case| case.source["id"] == "hevc-sdr-8-mp4")
                .ok_or_else(|| anyhow::anyhow!("H264 copy boundary reference unavailable"))?;
            self.source_copy_boundary = Some(
                source_copy_boundary::run(fixture, state, root, tools, &reference.path).await?,
            );
            self.source_recovery
                .prepare(fixture, source, root, tools)
                .await?;
        }
        Ok(())
    }

    pub(super) fn source_recovery_ready(&self) -> Value {
        self.source_recovery.ready()
    }

    pub(super) async fn poll(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        root: &Path,
        tools: &process::FixtureProcesses,
        resource: Uuid,
    ) -> anyhow::Result<()> {
        for client in ["browser", "native"] {
            for lane in if source_enabled() {
                vec!["format", "source"]
            } else {
                vec!["format"]
            } {
                for index in 0..self.cases.len() {
                    let phase = if lane == "format" {
                        self.cases[index].phase(client)
                    } else {
                        format!("source-{client}-{index:02}")
                    };
                    if self.completed.contains_key(&phase) {
                        continue;
                    }
                    let Some(observed) =
                        super::read_record(&format!("/tmp/direct-observed-{phase}.json")).await?
                    else {
                        break;
                    };
                    anyhow::ensure!(
                        observed["resourceId"] == resource.to_string()
                            && observed["phase"] == phase
                    );
                    self.closed(state, fixture).await?;
                    let checked = if lane == "format" {
                        verify::run(self, fixture, state, root, tools, index, client, &observed)
                            .await
                    } else {
                        source_verify::run(
                            self, fixture, state, root, tools, index, client, &observed,
                        )
                        .await
                    };
                    let response = match &checked {
                        Ok(backend) => {
                            json!({"resourceId":resource,"phase":phase,"passed":true,"backend":backend})
                        }
                        Err(_) => {
                            json!({"resourceId":resource,"phase":phase,"passed":false,"error":"Owned format verification failed."})
                        }
                    };
                    super::write_record(&format!("/tmp/direct-verified-{phase}"), &response)
                        .await?;
                    self.completed.insert(phase, checked?);
                    break;
                }
            }
        }
        if source_enabled() {
            self.source_recovery
                .poll(
                    fixture,
                    state,
                    root,
                    tools,
                    resource,
                    &self.peer_token,
                    &self.kids_token,
                )
                .await?;
        }
        Ok(())
    }

    pub(super) async fn finish(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(
            self.cases.len() == 14
                && self.completed.len() == if source_enabled() { 56 } else { 28 },
            "format client matrix is incomplete"
        );
        self.closed(state, fixture).await?;
        for case in &self.cases {
            anyhow::ensure!(inventory::hash(&case.path).await? == case.source["sha256"]);
        }
        let source_recovery = if source_enabled() {
            Some(self.source_recovery.finish(fixture, state).await?)
        } else {
            None
        };
        Ok(
            json!({"executed":true,"cases":14,"clients":2,"scanner_ingested":true,"sources_unchanged":true,"cleanup_passed":true,"source_capabilities":source_enabled(),"quick_scan":self.quick_scan,"source_copy_boundary":self.source_copy_boundary,"source_descriptors":self.source_descriptors,"source_recovery":source_recovery,"results":self.completed}),
        )
    }

    async fn closed(
        &self,
        state: &AppState,
        fixture: &ProgressiveFixture<'_>,
    ) -> anyhow::Result<()> {
        for play in &self.source_sessions {
            let stopped: bool =
                sqlx::query_scalar("SELECT stopped_at IS NOT NULL FROM play_sessions WHERE id=$1")
                    .bind(play)
                    .fetch_one(fixture.pool)
                    .await?;
            anyhow::ensure!(
                stopped,
                "source native Close did not stop its logical session"
            );
        }
        for (play, generation, directory) in &self.sessions {
            let stopped: bool =
                sqlx::query_scalar("SELECT stopped_at IS NOT NULL FROM play_sessions WHERE id=$1")
                    .bind(play)
                    .fetch_one(fixture.pool)
                    .await?;
            anyhow::ensure!(
                stopped
                    && state.transcode_manager.get_session(generation).is_none()
                    && !directory.exists(),
                "format Close did not retire the previous session and worker"
            );
        }
        Ok(())
    }

    pub(super) async fn cleanup(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<()> {
        let mut failures = Vec::new();
        if let Err(error) = self.source_recovery.cleanup(fixture, state).await {
            failures.push(error.to_string());
        }
        let plays = sqlx::query_scalar::<_, Uuid>(
            "SELECT id FROM play_sessions WHERE library_id=$1 AND stopped_at IS NULL",
        )
        .bind(self.library)
        .fetch_all(fixture.pool)
        .await;
        match plays {
            Ok(plays) => {
                for play in plays {
                    let stopped = tokio::time::timeout(
                        std::time::Duration::from_secs(10),
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
                        failures.push("format playback stop failed".to_string());
                    }
                }
            }
            Err(error) => failures.push(error.to_string()),
        }
        if state
            .transcode_manager
            .list_active_sessions()
            .iter()
            .any(|session| session.source_path.starts_with(&self.root))
        {
            failures.push("format worker remains after cleanup".into());
        }
        if let Err(error) = sqlx::query("DELETE FROM user_sessions WHERE id=$1 AND user_id=$2")
            .bind(self.kids_session)
            .bind(fixture.user)
            .execute(fixture.pool)
            .await
        {
            failures.push(error.to_string());
        }
        if let Err(error) = sqlx::query("DELETE FROM users WHERE id=$1")
            .bind(self.peer)
            .execute(fixture.pool)
            .await
        {
            failures.push(error.to_string());
        }
        if let Err(error) = sqlx::query("DELETE FROM libraries WHERE id=$1")
            .bind(self.library)
            .execute(fixture.pool)
            .await
        {
            failures.push(error.to_string());
        }
        anyhow::ensure!(
            failures.is_empty(),
            "format cleanup failed: {}",
            failures.join("; ")
        );
        Ok(())
    }
}
