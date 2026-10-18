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
    peer: Uuid,
    peer_token: String,
    kids_session: Uuid,
    kids_token: String,
}

pub(super) fn enabled() -> bool {
    std::env::var("DUSKCUE_TEST_FORMAT_CORPUS").as_deref() == Ok("live")
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

    pub(super) async fn poll(
        &mut self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
        root: &Path,
        tools: &process::FixtureProcesses,
        resource: Uuid,
    ) -> anyhow::Result<()> {
        for client in ["browser", "native"] {
            for index in 0..self.cases.len() {
                let phase = self.cases[index].phase(client);
                if self.completed.contains_key(&phase) {
                    continue;
                }
                let Some(observed) =
                    super::read_record(&format!("/tmp/direct-observed-{phase}.json")).await?
                else {
                    break;
                };
                anyhow::ensure!(
                    observed["resourceId"] == resource.to_string() && observed["phase"] == phase
                );
                self.closed(state, fixture).await?;
                let checked =
                    verify::run(self, fixture, state, root, tools, index, client, &observed).await;
                let response = match &checked {
                    Ok(backend) => {
                        json!({"resourceId":resource,"phase":phase,"passed":true,"backend":backend})
                    }
                    Err(_) => {
                        json!({"resourceId":resource,"phase":phase,"passed":false,"error":"Owned format verification failed."})
                    }
                };
                super::write_record(&format!("/tmp/direct-verified-{phase}"), &response).await?;
                self.completed.insert(phase, checked?);
                break;
            }
        }
        Ok(())
    }

    pub(super) async fn finish(
        &self,
        fixture: &ProgressiveFixture<'_>,
        state: &AppState,
    ) -> anyhow::Result<Value> {
        anyhow::ensure!(
            self.cases.len() == 14 && self.completed.len() == 28,
            "format client matrix is incomplete"
        );
        self.closed(state, fixture).await?;
        for case in &self.cases {
            anyhow::ensure!(inventory::hash(&case.path).await? == case.source["sha256"]);
        }
        Ok(
            json!({"executed":true,"cases":14,"clients":2,"scanner_ingested":true,"sources_unchanged":true,"cleanup_passed":true,"results":self.completed}),
        )
    }

    async fn closed(
        &self,
        state: &AppState,
        fixture: &ProgressiveFixture<'_>,
    ) -> anyhow::Result<()> {
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
