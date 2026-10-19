// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::collections::BTreeSet;
use std::path::Path;

use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use uuid::Uuid;

use super::{AppState, ProgressiveFixture, policy};

struct Canonical {
    metadata: Value,
    generations: BTreeSet<Uuid>,
    seek_events: i64,
}

async fn snapshot(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    play: Uuid,
) -> anyhow::Result<Canonical> {
    let metadata: Value =
        sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1 AND stopped_at IS NULL")
            .bind(play)
            .fetch_one(fixture.pool)
            .await?;
    let seek_events = sqlx::query_scalar(
        "SELECT count(*) FROM play_events WHERE play_session_id=$1 AND event_type='seek'",
    )
    .bind(play)
    .fetch_one(fixture.pool)
    .await?;
    Ok(Canonical {
        metadata,
        generations: state
            .transcode_manager
            .list_active_sessions()
            .iter()
            .map(|session| session.id)
            .collect(),
        seek_events,
    })
}

async fn unchanged(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    play: Uuid,
    generation: Uuid,
    directory: &Path,
    before: &Canonical,
) -> anyhow::Result<()> {
    let after = snapshot(fixture, state, play).await?;
    for field in [
        "transcode_session_id",
        "stream_offset_ms",
        "source_route_context",
        "recovery_budget",
        "recovery",
        "media_file_id",
    ] {
        anyhow::ensure!(
            after.metadata[field] == before.metadata[field],
            "refused source seek changed canonical {field}"
        );
    }
    let previous = before.metadata["current_position_ms"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("source seek checkpoint missing"))?;
    let current = after.metadata["current_position_ms"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("source seek checkpoint disappeared"))?;
    anyhow::ensure!(
        previous >= 5000
            && current >= previous
            && current <= 8000
            && current != 1000
            && after.seek_events == before.seek_events
            && after.generations == before.generations,
        "refused source seek wrote its requested checkpoint, emitted a seek, or allocated a generation"
    );
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("refused source seek retired its healthy generation"))?;
    anyhow::ensure!(
        session.segment_dir == directory
            && directory.is_dir()
            && state
                .transcode_manager
                .session_failure(generation)
                .is_none(),
        "refused source seek removed its healthy worker/cache"
    );
    Ok(())
}

async fn refused(client: &reqwest::Client, headers: &HeaderMap, play: Uuid) -> anyhow::Result<()> {
    let response = client
        .post(format!("{}/api/v1/playback/seek", policy::ORIGIN))
        .headers(headers.clone())
        .json(&json!({"session_id":play,"position_ms":1000}))
        .send()
        .await?;
    anyhow::ensure!(
        response.status() == reqwest::StatusCode::NOT_FOUND,
        "source seek unexpectedly survived profile/source revocation"
    );
    let body: Value = response.json().await?;
    anyhow::ensure!(body["title"] == "PLAY_001");
    Ok(())
}

pub(super) async fn run(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    client: &reqwest::Client,
    owner_headers: &HeaderMap,
    play: Uuid,
    file: Uuid,
    generation: Uuid,
    directory: &Path,
    kids_token: &str,
) -> anyhow::Result<Value> {
    let before = snapshot(fixture, state, play).await?;
    let mut kids = HeaderMap::new();
    kids.insert(
        reqwest::header::AUTHORIZATION,
        format!("Bearer {kids_token}").parse()?,
    );
    refused(client, &kids, play).await?;
    unchanged(fixture, state, play, generation, directory, &before).await?;

    let original: (bool, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT is_healthy,updated_at FROM media_files WHERE id=$1")
            .bind(file)
            .fetch_one(fixture.pool)
            .await?;
    anyhow::ensure!(original.0);
    let mut mutation = fixture.pool.begin().await?;
    let changed =
        sqlx::query("UPDATE media_files SET is_healthy=false WHERE id=$1 AND is_healthy=true")
            .bind(file)
            .execute(&mut *mutation)
            .await?;
    anyhow::ensure!(changed.rows_affected() == 1);
    mutation.commit().await?;
    let checked = async {
        let before = snapshot(fixture, state, play).await?;
        refused(client, owner_headers, play).await?;
        unchanged(fixture, state, play, generation, directory, &before).await
    }
    .await;
    let mut restoration = fixture.pool.begin().await?;
    let restored = sqlx::query("UPDATE media_files SET is_healthy=$2,updated_at=$3 WHERE id=$1")
        .bind(file)
        .bind(original.0)
        .bind(original.1)
        .execute(&mut *restoration)
        .await?;
    anyhow::ensure!(restored.rows_affected() == 1);
    restoration.commit().await?;
    let restored: (bool, chrono::DateTime<chrono::Utc>) =
        sqlx::query_as("SELECT is_healthy,updated_at FROM media_files WHERE id=$1")
            .bind(file)
            .fetch_one(fixture.pool)
            .await?;
    anyhow::ensure!(
        restored == original,
        "owned source health/identity was not restored after refused seek"
    );
    checked?;
    Ok(
        json!({"same_user_kids_status":404,"unhealthy_source_status":404,"error_code":"PLAY_001","canonical_generation_preserved":true,
        "requested_checkpoint_not_written":true,"no_seek_event":true,"recovery_budget_preserved":true,"no_generation_allocated":true,
        "healthy_worker_and_cache_preserved":true,"file_health_and_identity_restored":true}),
    )
}
