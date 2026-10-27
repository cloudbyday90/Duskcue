// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{database::Database, support::Fixture};
use reqwest::{Method, StatusCode};
use serde_json::json;
use uuid::Uuid;

pub(super) async fn direct(db: &Database) -> anyhow::Result<()> {
    let mut fixture = Fixture::new(db).await?;
    let result = direct_case(&fixture).await;
    let cleanup = fixture.close().await;
    result?;
    cleanup
}
async fn direct_case(f: &Fixture) -> anyhow::Result<()> {
    let id = Uuid::now_v7();
    let accepted = f.start(id, false, false).await?;
    anyhow::ensure!(
        accepted["status"] == "preparing"
            && accepted["stream_url"].is_null()
            && accepted["ready"].is_null()
    );
    let ready = f.ready(id).await?;
    anyhow::ensure!(ready["ready"]["delivery_mode"] == "direct_play");
    let deadline: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1")
            .bind(id)
            .fetch_one(&f.base.state.pool)
            .await?;
    f.start(id, false, false).await?;
    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT deadline FROM playback_reservations WHERE id=$1")
            .bind(id)
            .fetch_one(&f.base.state.pool)
            .await?;
    anyhow::ensure!(deadline == after);
    let changed = f
        .json(
            Method::POST,
            "/api/v1/playback-commands",
            Some(f.start_body(id, true, false)),
        )
        .await?;
    anyhow::ensure!(changed.0 == StatusCode::CONFLICT);
    let output = ready["stream_url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("owned output URI required"))?;
    let response = f
        .client
        .get(format!("{}{output}", f.url))
        .header("cookie", format!("session={}", f.token))
        .header("range", "bytes=1-100")
        .send()
        .await?;
    anyhow::ensure!(response.status() == StatusCode::PARTIAL_CONTENT);
    let bytes = response.bytes().await?;
    let source = tokio::fs::read(&f.source).await?;
    anyhow::ensure!(bytes.as_ref() == &source[1..101]);
    let wrong = f.response(Method::GET, output, None, true).await?;
    anyhow::ensure!(wrong.status() == StatusCode::CONFLICT);
    let session = ready["ready"]["session_id"].as_str().unwrap();
    for (method, path, body) in [
        (
            Method::GET,
            format!("/api/v1/playback/info/{session}"),
            None,
        ),
        (
            Method::POST,
            "/api/v1/playback/heartbeat".into(),
            Some(json!({"session_id":session,"position_ms":1000})),
        ),
        (
            Method::POST,
            "/api/v1/playback/stop".into(),
            Some(json!({"session_id":session})),
        ),
        (
            Method::POST,
            "/api/v1/playback/seek".into(),
            Some(json!({"session_id":session,"position_ms":1000})),
        ),
    ] {
        let response = f.response(method, &path, body, false).await?;
        anyhow::ensure!(response.status() == StatusCode::NOT_FOUND);
    }
    let history: serde_json::Value =
        sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1::uuid")
            .bind(session)
            .fetch_one(&f.base.state.pool)
            .await?;
    anyhow::ensure!(
        !history
            .to_string()
            .contains(f.source.to_string_lossy().as_ref())
    );
    f.stop(id).await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM playback_reservations WHERE household_id=$1 AND state<>'released'",
    )
    .bind(f.base.actor.user)
    .fetch_one(&f.base.state.pool)
    .await?;
    anyhow::ensure!(count == 0);
    let replay = f.start(id, false, false).await?;
    anyhow::ensure!(replay["status"] == "released" && replay["stream_url"].is_null());
    Ok(())
}

pub(super) async fn hls(db: &Database) -> anyhow::Result<()> {
    let mut fixture = Fixture::new(db).await?;
    let result = hls_case(&fixture).await;
    let cleanup = fixture.close().await;
    result?;
    cleanup
}
async fn hls_case(f: &Fixture) -> anyhow::Result<()> {
    let id = Uuid::now_v7();
    f.start(id, true, false).await?;
    let ready = f.ready(id).await?;
    anyhow::ensure!(ready["ready"]["delivery_mode"] == "direct_stream");
    let response = f
        .response(
            Method::GET,
            ready["stream_url"].as_str().unwrap(),
            None,
            false,
        )
        .await?;
    anyhow::ensure!(response.status() == StatusCode::OK);
    let manifest = response.text().await?;
    anyhow::ensure!(!manifest.contains("/api/v1/transcode/") && manifest.starts_with("#EXTM3U"));
    let urls: Vec<&str> = manifest
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .collect();
    anyhow::ensure!(!urls.is_empty());
    let text = if manifest.contains("#EXT-X-STREAM-INF:") {
        let playlist = f.response(Method::GET, urls[0], None, false).await?;
        anyhow::ensure!(playlist.status() == StatusCode::OK);
        playlist.text().await?
    } else {
        manifest
    };
    let init = text
        .split("URI=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .ok_or_else(|| anyhow::anyhow!("initialization URI required"))?;
    let segment = text
        .lines()
        .find(|line| !line.starts_with('#') && !line.is_empty())
        .ok_or_else(|| anyhow::anyhow!("segment URI required"))?;
    for url in [init, segment] {
        let response = f.response(Method::GET, url, None, false).await?;
        anyhow::ensure!(response.status() == StatusCode::OK && !response.bytes().await?.is_empty());
    }
    let generation: Uuid = sqlx::query_scalar(
        "SELECT generation_id FROM playback_vod_commands WHERE reservation_id=$1",
    )
    .bind(id)
    .fetch_one(&f.base.state.pool)
    .await?;
    let legacy = f
        .response(
            Method::GET,
            &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
            None,
            false,
        )
        .await?;
    anyhow::ensure!(legacy.status() == StatusCode::NOT_FOUND);
    let encoded: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM playback_vod_commands WHERE reservation_id=$1 AND encodes",
    )
    .bind(id)
    .fetch_one(&f.base.state.pool)
    .await?;
    anyhow::ensure!(encoded == 0);
    f.stop(id).await?;
    anyhow::ensure!(
        f.base
            .state
            .transcode_manager
            .get_session(&generation)
            .is_none()
    );
    Ok(())
}

pub(super) async fn policy(db: &Database) -> anyhow::Result<()> {
    let mut fixture = Fixture::new(db).await?;
    let result = policy_case(&fixture).await;
    let cleanup = fixture.close().await;
    result?;
    cleanup
}
async fn policy_case(f: &Fixture) -> anyhow::Result<()> {
    let id = Uuid::now_v7();
    f.start(id, false, false).await?;
    f.ready(id).await?;
    sqlx::query("UPDATE streaming_policies SET max_streams=0,max_transcode_streams=0,bandwidth_limit_bps=1,max_transcode_resolution='480p' WHERE id=$1").bind(f.policy).execute(&f.base.state.pool).await?;
    let renewed = f
        .json(
            Method::POST,
            &format!("/api/v1/playback-commands/{id}/renew"),
            Some(json!({})),
        )
        .await?;
    anyhow::ensure!(renewed.0 == StatusCode::OK && renewed.1["status"] == "renewed");
    sqlx::query("UPDATE streaming_policies SET allow_direct_play=false WHERE id=$1")
        .bind(f.policy)
        .execute(&f.base.state.pool)
        .await?;
    sqlx::query("UPDATE streaming_policies SET allow_direct_play=true WHERE id=$1")
        .bind(f.policy)
        .execute(&f.base.state.pool)
        .await?;
    let state: String = sqlx::query_scalar("SELECT state FROM playback_reservations WHERE id=$1")
        .bind(id)
        .fetch_one(&f.base.state.pool)
        .await?;
    anyhow::ensure!(state != "held");
    f.stop(id).await?;
    Ok(())
}
