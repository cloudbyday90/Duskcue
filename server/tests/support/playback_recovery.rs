// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::time::Duration;

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::{Extension, Router};
use duskcue::domains::{playback, profiles};
use duskcue::services::encryption::EncryptionKey;
use duskcue::services::transcoding::FailureClass;
use duskcue::state::{AppState, RuntimeConfig};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Evidence, ProgressiveFixture, Stream, cleanup, enabled, media, observe, parse_uuid};
use super::{post, process, start, switch};

#[path = "profile_access_connection.rs"]
mod access_connection;
#[path = "playback_recovery_process.rs"]
mod injection;
#[path = "playback_recovery_races.rs"]
mod races;

pub(super) async fn run(fixture: ProgressiveFixture<'_>) -> anyhow::Result<()> {
    anyhow::ensure!(
        enabled()
            && std::env::var("DUSKCUE_TEST_FFMPEG_MODE").as_deref() == Ok("linux-local-managed"),
        "requires the owned managed recovery qualification"
    );
    let resource = Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    let parent = fixture.state.bootstrap.data_dir.canonicalize()?;
    let root = parent.join(format!("progressive-http-recovery-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&root).await?;
    anyhow::ensure!(root.canonicalize()?.parent() == Some(parent.as_path()));
    let mut bootstrap = fixture.state.bootstrap.clone();
    bootstrap.data_dir = root.join("data");
    bootstrap.cache_dir = root.join("cache");
    let mut config = RuntimeConfig::default();
    config.cpu.ffmpeg_threads = Some(1);
    config.cpu.hw_accel_auto_detect = false;
    config.resource_limits.max_concurrent_transcodes = 1;
    config.resource_limits.ffmpeg_shutdown_grace_secs = 1;
    config.transcoding.segment_duration_seconds = 2;
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (key, _) = EncryptionKey::generate();
    let state = AppState::new_with_config(
        fixture.pool.clone(),
        bootstrap,
        config,
        recorder.handle(),
        key,
    );
    let app = playback::router(state.clone())
        .merge(profiles::router(state.clone()))
        .with_state(state.clone())
        .layer(Extension(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            48027,
        )))));
    let mut evidence = Evidence {
        root,
        item: Uuid::now_v7(),
        file: Uuid::now_v7(),
        sessions: Vec::new(),
        source_seconds: None,
        source_bytes: None,
        first_segments: None,
        seek_first_segments: None,
        completed_segments: None,
        profile_refused: false,
        old_seek_released: false,
        permit_reused: false,
        tools: process::FixtureProcesses::default(),
    };
    let proof = tokio::time::timeout(
        Duration::from_secs(60),
        exercise(&fixture, &state, &app, &mut evidence),
    )
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    let cleaned = cleanup(&fixture, &state, &app, &evidence).await;
    println!(
        "\nDUSKCUE_PLAYBACK_RECOVERY={}",
        json!({
            "version": 1, "executed": true, "passed": proof.is_ok() && cleaned.is_ok(),
            "resource_id": resource,
            "source_seconds": evidence.source_seconds, "source_bytes": evidence.source_bytes,
            "real_process_stall": proof.is_ok(),
            "healthy_hint_did_not_restart": proof.is_ok(),
            "generation_replay_preserved_position_budget": proof.is_ok(),
            "seek_and_stop_serialized_with_heartbeat": proof.is_ok(),
            "recovery_kept_original_profile": proof.is_ok(),
            "cleanup_passed": cleaned.is_ok(),
            "fixture_children_confirmed_exited": evidence.tools.exited(),
            "encoder_identities": evidence.sessions.iter().map(|session| session.encoder.recorded_identity()).collect::<Vec<_>>(),
            "decoded_stream_relative_timestamps": evidence.sessions.iter().map(|session| session.timestamp).collect::<Vec<_>>(),
            "browser_to_live_server": "unqualified"
        })
    );
    proof?;
    cleaned
}

async fn exercise(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    evidence: &mut Evidence,
) -> anyhow::Result<()> {
    let source = media::source(
        &evidence.tools,
        fixture.source,
        &evidence.root.join("input"),
    )
    .await?;
    evidence.source_seconds = Some(source.probed_seconds);
    evidence.source_bytes = Some(source.bytes);
    sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','Recovery fixture','Recovery fixture','R')")
        .bind(evidence.item).bind(fixture.library).execute(fixture.pool).await?;
    sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,runtime_seconds,video_codec,video_resolution,video_bitrate,audio_codec,audio_channels,additional_streams) VALUES ($1,$2,$3,$4,'matroska',$5,'h264','320x180',400000,'aac',1,$6)")
        .bind(evidence.file).bind(evidence.item).bind(source.path.to_string_lossy().as_ref()).bind(i64::try_from(source.bytes)?)
        .bind(source.runtime_seconds).bind(source.audio).execute(fixture.pool).await?;
    access_connection::run(
        fixture.pool,
        fixture.user,
        fixture.original_profile,
        evidence.item,
    )
    .await?;
    let stream = Stream {
        state,
        app,
        token: fixture.token,
        source: &source.path,
        root: &evidence.root,
        tools: &evidence.tools,
    };
    let first = start(&stream, evidence.item, evidence.file).await?;
    let play = first.play;
    let original = first.transcode.id;
    let original_cache = first.transcode.segment_dir.clone();
    let original_encoder = first.encoder.clone();
    evidence.sessions.push(first);
    let healthy = heartbeat(app, fixture.token, play, original, 10_000).await?;
    anyhow::ensure!(
        healthy.0 == StatusCode::OK
            && healthy.1["recovery"]["status"] == "exhausted"
            && healthy.1["recovery"]["reason"] == "not_eligible"
            && state.transcode_manager.active_session_count() == 1,
        "a healthy client hint unexpectedly restarted the server worker"
    );
    let before = metadata(fixture.pool, play).await?;
    anyhow::ensure!(
        before["recovery_budget"]["attempts"].as_u64().unwrap_or(0) == 0
            && before["recovery_budget"]["exhausted"] != true,
        "a healthy client hint spent or exhausted the persistent retry budget"
    );
    let frozen = injection::FrozenEncoder::stop(&original_encoder)?;
    tokio::time::timeout(Duration::from_secs(36), async {
        loop {
            match state.transcode_manager.session_failure(original) {
                Some(FailureClass::Stalled) => return Ok::<_, anyhow::Error>(()),
                Some(other) => anyhow::bail!("stopped fixture classified as {other:?}"),
                None => tokio::time::sleep(Duration::from_millis(25)).await,
            }
        }
    })
    .await??;
    let peer = heartbeat(app, fixture.peer_token, play, original, 590_000).await?;
    anyhow::ensure!(peer.0 == StatusCode::NOT_FOUND);
    switch(app, fixture.token, fixture.kids_profile).await?;
    let wrong_profile = heartbeat(app, fixture.token, play, original, 590_000).await?;
    anyhow::ensure!(wrong_profile.0 == StatusCode::NOT_FOUND);
    let denied = metadata(fixture.pool, play).await?;
    anyhow::ensure!(denied["recovery_budget"]["attempts"].as_u64().unwrap_or(0) == 0);
    switch(app, fixture.token, fixture.original_profile).await?;
    let recovered = heartbeat(app, fixture.token, play, original, 10_000).await?;
    anyhow::ensure!(
        recovered.0 == StatusCode::OK
            && recovered.1["recovery"]["status"] == "recovered"
            && recovered.1["recovery"]["attempts"] == 1
            && recovered.1["recovery"]["skipped_ms"] == 0
            && recovered.1["stream_offset_ms"] == 10_000,
        "observed real stall did not produce a bounded source-position recovery"
    );
    let replacement = parse_uuid(&recovered.1, "transcode_session_id")?;
    anyhow::ensure!(replacement != original);
    original_encoder.gone()?;
    drop(frozen);
    anyhow::ensure!(
        !original_cache.exists() && state.transcode_manager.get_session(&original).is_none(),
        "recovery returned before releasing the old physical generation"
    );
    let bound: (Uuid, Uuid) =
        sqlx::query_as("SELECT user_id,profile_id FROM play_sessions WHERE id=$1")
            .bind(play)
            .fetch_one(fixture.pool)
            .await?;
    anyhow::ensure!(bound == (fixture.user, fixture.original_profile));
    let second = observe(&stream, play, replacement, Some(10_000)).await?;
    anyhow::ensure!(second.transcode.source_audio_stream_index == Some(2));
    evidence.sessions.push(second);
    let stale = heartbeat(app, fixture.token, play, original, 590_000).await?;
    canonical(&stale, replacement, 10_000, 10_000)?;
    let missing = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        json!({ "session_id": play, "position_ms": 590_000, "state": "playing", "recovery_requested": true }),
    )
    .await?;
    canonical(&missing, replacement, 10_000, 10_000)?;
    let stored = metadata(fixture.pool, play).await?;
    anyhow::ensure!(
        stored["recovery_budget"]["attempts"] == 1 && stored["current_position_ms"] == 10_000,
        "stale generation changed the source position or retry budget"
    );
    races::exercise(fixture, &stream, &mut evidence.sessions, play, replacement).await?;
    Ok(())
}

async fn heartbeat(
    app: &Router,
    token: &str,
    play: Uuid,
    generation: Uuid,
    position: i32,
) -> anyhow::Result<(StatusCode, Value)> {
    post(
        app,
        token,
        "/api/v1/playback/heartbeat",
        heartbeat_body(play, generation, position),
    )
    .await
}

fn heartbeat_body(play: Uuid, generation: Uuid, position: i32) -> Value {
    json!({ "session_id": play, "position_ms": position, "state": "playing", "transcode_session_id": generation, "recovery_requested": true })
}

fn canonical(
    response: &(StatusCode, Value),
    generation: Uuid,
    position: i32,
    offset: i32,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        response.0 == StatusCode::OK
            && response.1["transcode_session_id"] == generation.to_string()
            && response.1["stream_url"] == format!("/api/v1/transcode/{generation}/manifest.m3u8")
            && response.1["stream_offset_ms"] == offset
            && response.1["position_ms"] == position,
        "stale heartbeat did not return the unchanged canonical generation/position"
    );
    Ok(())
}

async fn metadata(pool: &sqlx::PgPool, play: Uuid) -> anyhow::Result<Value> {
    Ok(
        sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1")
            .bind(play)
            .fetch_one(pool)
            .await?,
    )
}
