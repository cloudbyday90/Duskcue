// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::Router;
use axum::http::StatusCode;
use serde_json::{Value, json};
use tokio::task::JoinSet;
use uuid::Uuid;

use super::super::Session;
use super::{ProgressiveFixture, Stream, canonical, heartbeat_body, metadata, observe, post};

pub(super) async fn exercise(
    fixture: &ProgressiveFixture<'_>,
    stream: &Stream<'_>,
    sessions: &mut Vec<Session>,
    play: Uuid,
    previous: Uuid,
) -> anyhow::Result<()> {
    let serial = Serial {
        pool: fixture.pool,
        app: stream.app,
        token: fixture.token,
        play,
    };
    let (seek, stale) = serial
        .run(
            "/api/v1/playback/seek",
            30_000,
            heartbeat_body(play, previous, 590_000),
        )
        .await?;
    anyhow::ensure!(seek.0 == StatusCode::OK);
    let current = super::parse_uuid(&seek.1, "transcode_session_id")?;
    anyhow::ensure!(current != previous);
    canonical(&stale, current, 30_000, 30_000)?;
    let stored = metadata(fixture.pool, play).await?;
    anyhow::ensure!(stored["recovery_budget"]["attempts"] == 1);
    let old = sessions
        .last()
        .ok_or_else(|| anyhow::anyhow!("recovery evidence missing"))?;
    old.encoder.gone()?;
    anyhow::ensure!(!old.transcode.segment_dir.exists());
    let sought = observe(stream, play, current, Some(30_000)).await?;
    sessions.push(sought);
    let (stopped, heartbeat) = serial
        .run(
            "/api/v1/playback/stop",
            30_000,
            heartbeat_body(play, current, 590_000),
        )
        .await?;
    anyhow::ensure!(stopped.0 == StatusCode::OK && heartbeat.0 == StatusCode::NOT_FOUND);
    let stopped = sessions
        .last()
        .ok_or_else(|| anyhow::anyhow!("seek evidence missing"))?;
    stopped.encoder.gone()?;
    anyhow::ensure!(
        !stopped.transcode.segment_dir.exists()
            && stream.state.transcode_manager.active_session_count() == 0,
        "stop/recovery serialization leaked a physical worker or cache"
    );
    let metadata = metadata(fixture.pool, play).await?;
    anyhow::ensure!(
        metadata["current_state"] == "stopped" && metadata["recovery_budget"]["attempts"] == 1,
        "late recovery changed stopped state or spent another attempt"
    );
    Ok(())
}

struct Serial<'a> {
    pool: &'a sqlx::PgPool,
    app: &'a Router,
    token: &'a str,
    play: Uuid,
}

impl Serial<'_> {
    async fn run(
        &self,
        first_path: &'static str,
        first_position: i32,
        heartbeat: Value,
    ) -> anyhow::Result<((StatusCode, Value), (StatusCode, Value))> {
        let mut blocker = self.pool.begin().await?;
        sqlx::query("SELECT id FROM play_sessions WHERE id=$1 FOR UPDATE")
            .bind(self.play)
            .fetch_one(&mut *blocker)
            .await?;
        let mut pending = JoinSet::new();
        let app = self.app.clone();
        let token = self.token.to_string();
        let play = self.play;
        pending.spawn(async move {
            (
                true,
                post(
                    &app,
                    &token,
                    first_path,
                    json!({ "session_id": play, "position_ms": first_position }),
                )
                .await,
            )
        });
        super::super::super::wait_for_lock(self.pool, 1).await?;
        let app = self.app.clone();
        let token = self.token.to_string();
        pending.spawn(async move {
            (
                false,
                post(&app, &token, "/api/v1/playback/heartbeat", heartbeat).await,
            )
        });
        super::super::super::wait_for_lock(self.pool, 2).await?;
        blocker.commit().await?;
        let mut first = None;
        let mut second = None;
        while let Some(response) = pending.join_next().await {
            let (is_first, response) = response?;
            if is_first {
                first = Some(response?);
            } else {
                second = Some(response?);
            }
        }
        Ok((
            first.ok_or_else(|| anyhow::anyhow!("serialized first request disappeared"))?,
            second.ok_or_else(|| anyhow::anyhow!("serialized heartbeat disappeared"))?,
        ))
    }
}
