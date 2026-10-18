// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;
use std::time::Duration;

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::{Extension, Router};
use duskcue::domains::{playback, profiles};
use duskcue::services::encryption::EncryptionKey;
use duskcue::services::transcoding::plan::StreamAction;
use duskcue::services::transcoding::{HwAccelMethod, OutputProfile};
use duskcue::state::{AppState, RuntimeConfig};
use serde_json::{Value, json};
use tokio::task::JoinSet;
use uuid::Uuid;

use super::{
    Evidence, ProgressiveFixture, cleanup, enabled, media, parse_uuid, post, process, switch,
};

#[path = "playback_direct_browser.rs"]
mod browser;

#[path = "playback_direct_media.rs"]
mod direct_media;
#[path = "playback_direct_native.rs"]
mod native;
#[path = "playback_direct_protocol.rs"]
mod protocol;
use direct_media::{clean_source, file_hash, output_media, seed};
struct DirectEvidence {
    owned: Evidence,
    proof: Value,
    browser: Value,
    trigger: bool,
    native: Option<native::Candidate>,
}

pub(super) async fn run(fixture: ProgressiveFixture<'_>) -> anyhow::Result<()> {
    anyhow::ensure!(
        enabled()
            && std::env::var("DUSKCUE_TEST_FFMPEG_MODE").as_deref() == Ok("linux-local-managed"),
        "requires the owned direct fallback qualification"
    );
    let resource = Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    let parent = fixture.state.bootstrap.data_dir.canonicalize()?;
    let root = parent.join(format!("progressive-http-direct-{}", Uuid::now_v7()));
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
    let mut state = AppState::new_with_config(
        fixture.pool.clone(),
        bootstrap,
        config,
        recorder.handle(),
        key,
    );
    let mut app = fixture_router(&state);
    let mut evidence = DirectEvidence {
        owned: Evidence {
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
        },
        proof: json!({}),
        browser: json!({"executed":false}),
        trigger: false,
        native: None,
    };
    let exercised = tokio::time::timeout(
        Duration::from_secs(300),
        exercise(&fixture, &mut state, &mut app, &mut evidence),
    )
    .await
    .map_err(anyhow::Error::from)
    .and_then(|result| result);
    let trigger_cleanup = drop_failure_trigger(fixture.pool, evidence.trigger).await;
    let native_cleanup = if let Some(candidate) = &evidence.native {
        native::cleanup(&fixture, &state, candidate).await
    } else {
        Ok(())
    };
    let cleaned = cleanup(&fixture, &state, &app, &evidence.owned).await;
    println!(
        "\nDUSKCUE_PLAYBACK_DIRECT={}",
        json!({
            "version":1,"executed":true,"passed":exercised.is_ok() && trigger_cleanup.is_ok() && native_cleanup.is_ok() && cleaned.is_ok(),
            "resource_id":resource,"cleanup_passed":trigger_cleanup.is_ok() && native_cleanup.is_ok() && cleaned.is_ok(),
            "fixture_children_confirmed_exited":evidence.owned.tools.exited(),
            "contracts":evidence.proof,"browser":evidence.browser,
            "source_seconds":evidence.owned.source_seconds,"source_bytes":evidence.owned.source_bytes
        })
    );
    exercised?;
    trigger_cleanup?;
    native_cleanup?;
    cleaned
}

fn fixture_router(state: &AppState) -> Router {
    playback::router(state.clone())
        .merge(profiles::router(state.clone()))
        .with_state(state.clone())
        .layer(Extension(ConnectInfo(std::net::SocketAddr::from((
            [127, 0, 0, 1],
            48027,
        )))))
}

async fn exercise(
    fixture: &ProgressiveFixture<'_>,
    state: &mut AppState,
    app: &mut Router,
    evidence: &mut DirectEvidence,
) -> anyhow::Result<()> {
    let original_hash = file_hash(fixture.source).await?;
    let source = clean_source(&evidence.owned.tools, fixture.source, &evidence.owned.root).await?;
    let hash = file_hash(&source).await?;
    evidence.proof["original_source_sha256"] = json!(original_hash);
    evidence.proof["clean_source_sha256"] = json!(hash);
    evidence.owned.source_seconds = Some(8.0);
    evidence.owned.source_bytes = Some(tokio::fs::metadata(&source).await?.len());
    seed(fixture, &evidence.owned, &source).await?;
    if native::enabled() {
        evidence.native =
            Some(native::prepare(&evidence.owned.tools, &source, &evidence.owned.root).await?);
        let candidate = evidence.native.as_ref().unwrap();
        direct_media::seed_source(
            fixture,
            &evidence.owned.tools,
            &candidate.path,
            candidate.item,
            candidate.file,
            60,
        )
        .await?;
        native::configure(state, &evidence.owned.root, candidate).await?;
        *app = fixture_router(state);
    }
    protocol::legacy_bookkeeping(fixture, state, app, evidence).await?;
    let start = start_direct(fixture, app, evidence.owned.item, evidence.owned.file).await?;
    let play = parse_uuid(&start, "session_id")?;
    let revision = parse_uuid(&start, "direct_source_revision")?;
    let descriptor = start["direct_fallback"].clone();
    anyhow::ensure!(state.transcode_manager.active_session_count() == 0);
    let ordinary = hint(play, Some(revision), 1000, false, true, "playing");
    let response = post(app, fixture.token, "/api/v1/playback/heartbeat", ordinary).await?;
    anyhow::ensure!(response.0 == StatusCode::OK && response.1["position_ms"] == 1000);
    let before = snapshot(fixture.pool, play).await?;
    for current in [None, Some(Uuid::now_v7())] {
        let response = post(
            app,
            fixture.token,
            "/api/v1/playback/heartbeat",
            hint(play, current, 7000, true, true, "paused"),
        )
        .await?;
        anyhow::ensure!(
            response.0 == StatusCode::OK
                && response.1["direct_source_revision"] == revision.to_string()
                && response.1["position_ms"] == 1000
        );
        anyhow::ensure!(
            snapshot(fixture.pool, play).await? == before,
            "missing/stale direct revision mutated checkpoint or events"
        );
    }
    let peer = post(
        app,
        fixture.peer_token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(revision), 7000, true, true, "playing"),
    )
    .await?;
    anyhow::ensure!(peer.0 == StatusCode::NOT_FOUND);
    switch(app, fixture.token, fixture.kids_profile).await?;
    let kids = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(revision), 7000, true, true, "playing"),
    )
    .await?;
    anyhow::ensure!(kids.0 == StatusCode::NOT_FOUND);
    let kids_info = super::super::request(
        app,
        fixture.token,
        "GET",
        &format!("/api/v1/playback/info/{play}"),
        json!({}),
    )
    .await?;
    anyhow::ensure!(kids_info.0 == StatusCode::NOT_FOUND);
    switch(app, fixture.token, fixture.original_profile).await?;
    anyhow::ensure!(snapshot(fixture.pool, play).await? == before);
    evidence.proof["revision_and_profile_fencing"] = json!(true);
    let sought = post(
        app,
        fixture.token,
        "/api/v1/playback/seek",
        json!({"session_id":play,"position_ms":1500}),
    )
    .await?;
    anyhow::ensure!(
        sought.0 == StatusCode::OK
            && sought.1["position_ms"] == 1500
            && sought.1["transcode_session_id"].is_null()
    );
    let current = parse_uuid(&sought.1, "direct_source_revision")?;
    anyhow::ensure!(current != revision);
    let before_seek_replay = snapshot(fixture.pool, play).await?;
    let stale = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(revision), 7000, true, true, "playing"),
    )
    .await?;
    anyhow::ensure!(
        stale.0 == StatusCode::OK
            && stale.1["direct_source_revision"] == current.to_string()
            && stale.1["position_ms"] == 1500
    );
    anyhow::ensure!(snapshot(fixture.pool, play).await? == before_seek_replay);
    evidence.proof["direct_seek_rotated_revision"] = json!(true);
    let mut pending = JoinSet::new();
    for _ in 0..2 {
        let app = app.clone();
        let token = fixture.token.to_owned();
        pending.spawn(async move {
            post(
                &app,
                &token,
                "/api/v1/playback/heartbeat",
                hint(play, Some(current), 2000, true, true, "paused"),
            )
            .await
        });
    }
    let mut responses = Vec::new();
    while let Some(response) = pending.join_next().await {
        responses.push(response??);
    }
    anyhow::ensure!(
        responses.len() == 2
            && responses
                .iter()
                .all(|response| response.0 == StatusCode::OK)
    );
    let promotion = &responses[0].1;
    let generation = parse_uuid(promotion, "transcode_session_id")?;
    for (_, response) in &responses {
        anyhow::ensure!(
            response["transcode_session_id"] == generation.to_string()
                && response["stream_decision"] == "transcode"
                && response["stream_offset_ms"] == 2000
                && response["position_ms"] == 2000
                && response["direct_source_revision"].is_null()
                && response["recovery"]["status"] == "recovered"
                && response["recovery"]["reason"] == "native_decode"
                && response["recovery"]["attempts"] == 1
                && response["recovery"]["skipped_ms"] == 0
                && response["recovery"]["quality_reduced"] == true,
            "duplicate direct promotion did not return one canonical admitted replacement: {response}"
        );
    }
    let session = state
        .transcode_manager
        .get_session(&generation)
        .ok_or_else(|| anyhow::anyhow!("promoted worker unavailable"))?;
    anyhow::ensure!(
        session.source_path == source
            && session.source_video_stream_index == Some(1)
            && session.source_audio_stream_index == Some(2)
            && session.output_profile == Some(OutputProfile::CompatibleSdr)
            && session.hw_accel == HwAccelMethod::Software
            && session.stream_plan.video == StreamAction::Encode
            && session.stream_plan.audio == StreamAction::Encode
            && session.recovery_attempts == 1
            && session.source_start_ms == 2000
            && session.target_video_resolution
                == (
                    descriptor["video"]["width"].as_u64().unwrap() as u32,
                    descriptor["video"]["height"].as_u64().unwrap() as u32
                )
            && session.target_bitrate == descriptor["video"]["bitrate"].as_u64().unwrap() as u32
    );
    let stored = snapshot(fixture.pool, play).await?;
    anyhow::ensure!(
        stored.0["current_state"] == "paused" && stored.0["recovery_budget"]["attempts"] == 1
    );
    let bound: (Uuid, Uuid) =
        sqlx::query_as("SELECT user_id,profile_id FROM play_sessions WHERE id=$1")
            .bind(play)
            .fetch_one(fixture.pool)
            .await?;
    anyhow::ensure!(bound == (fixture.user, fixture.original_profile));
    let duplicate = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(current), 7000, true, true, "playing"),
    )
    .await?;
    anyhow::ensure!(
        duplicate.0 == StatusCode::OK
            && duplicate.1["transcode_session_id"] == generation.to_string()
            && duplicate.1["position_ms"] == 2000
    );
    anyhow::ensure!(
        snapshot(fixture.pool, play).await? == stored,
        "old direct revision changed HLS checkpoint/budget after promotion"
    );
    evidence.proof["concurrent_promotion_one_attempt_same_session"] = json!(true);
    evidence.proof["encoded_media"] =
        output_media(app, fixture.token, generation, &evidence.owned).await?;
    let directory = session.segment_dir.clone();
    stop(app, fixture.token, play).await?;
    anyhow::ensure!(
        state.transcode_manager.get_session(&generation).is_none() && !directory.exists()
    );
    let late = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(current), 2000, true, true, "paused"),
    )
    .await?;
    anyhow::ensure!(
        late.0 == StatusCode::NOT_FOUND && state.transcode_manager.active_session_count() == 0
    );
    evidence.proof["stop_released_owned_generation"] = json!(true);
    refusal_cases(fixture, state, app, evidence).await?;
    rollback_case(fixture, state, app, evidence).await?;
    anyhow::ensure!(
        file_hash(&source).await? == hash && file_hash(fixture.source).await? == original_hash,
        "direct fallback modified an original source"
    );
    media::cache_bytes(&evidence.owned.root)?;
    if std::env::var("DUSKCUE_TEST_DIRECT_BROWSER").as_deref() == Ok("live") {
        evidence.browser = browser::run(
            fixture,
            state,
            &evidence.owned.root,
            &source,
            &evidence.owned.tools,
            evidence.native.as_ref(),
        )
        .await?;
    }
    Ok(())
}

async fn start_direct(
    fixture: &ProgressiveFixture<'_>,
    app: &Router,
    item: Uuid,
    file: Uuid,
) -> anyhow::Result<Value> {
    let (status, body) = post(
        app,
        fixture.token,
        "/api/v1/playback/start",
        direct_start_request(item, file),
    )
    .await?;
    anyhow::ensure!(
        status == StatusCode::OK
            && body["stream_decision"] == "direct_play"
            && body["transcode_session_id"].is_null()
            && body["stream_url"] == format!("/api/v1/stream/{file}")
            && body["selected_audio_stream_index"] == 2,
        "clean fixture did not receive native direct admission: {body}"
    );
    parse_uuid(&body, "direct_source_revision")?;
    anyhow::ensure!(
        body["direct_fallback"]["video"]["content_type"] == "video/mp4; codecs=\"avc1.640028\""
            && body["direct_fallback"]["video"]["framerate"] == 30
            && body["direct_fallback"]["video"]["width"] == 320
            && body["direct_fallback"]["video"]["height"] == 180
            && body["direct_fallback"]["audio"]["content_type"]
                == "audio/mp4; codecs=\"mp4a.40.2\""
            && body["direct_fallback"]["audio"]["samplerate"] == 48000,
        "direct capability descriptor absent or inconsistent: {body}"
    );
    Ok(body)
}

fn direct_start_request(item: Uuid, file: Uuid) -> Value {
    json!({
        "media_item_id":item,"media_file_id":file,"audio_stream_index":2,"playback_mode":"interactive","quality_mode":"balanced","max_streaming_bitrate":1000000,"direct_recovery_protocol":1,
        "device_profile":{"video_codecs":["h264"],"audio_codecs":["aac"],"containers":["mp4"],"max_resolution":"1080p","max_audio_channels":2,"max_bitrate_bps":1000000,"max_video_bit_depth":8}
    })
}

fn hint(
    play: Uuid,
    revision: Option<Uuid>,
    position: i32,
    requested: bool,
    supported: bool,
    state: &str,
) -> Value {
    json!({"session_id":play,"direct_source_revision":revision,"position_ms":position,"state":state,"is_paused":state == "paused","recovery_requested":requested,"direct_fallback_supported":supported})
}

async fn snapshot(pool: &sqlx::PgPool, play: Uuid) -> anyhow::Result<(Value, i64)> {
    let metadata = sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1")
        .bind(play)
        .fetch_one(pool)
        .await?;
    let events = sqlx::query_scalar("SELECT count(*) FROM play_events WHERE play_session_id=$1")
        .bind(play)
        .fetch_one(pool)
        .await?;
    Ok((metadata, events))
}

async fn stop(app: &Router, token: &str, play: Uuid) -> anyhow::Result<()> {
    let stopped = post(
        app,
        token,
        "/api/v1/playback/stop",
        json!({"session_id":play,"position_ms":2000}),
    )
    .await?;
    anyhow::ensure!(stopped.0 == StatusCode::OK);
    Ok(())
}

async fn refusal_cases(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    evidence: &mut DirectEvidence,
) -> anyhow::Result<()> {
    for reason in ["output_unsupported", "changed", "unhealthy"] {
        let start = start_direct(fixture, app, evidence.owned.item, evidence.owned.file).await?;
        let play = parse_uuid(&start, "session_id")?;
        let revision = parse_uuid(&start, "direct_source_revision")?;
        if reason == "changed" {
            sqlx::query("UPDATE media_files SET file_size=file_size+1 WHERE id=$1")
                .bind(evidence.owned.file)
                .execute(fixture.pool)
                .await?;
        } else if reason == "unhealthy" {
            sqlx::query("UPDATE media_files SET is_healthy=false WHERE id=$1")
                .bind(evidence.owned.file)
                .execute(fixture.pool)
                .await?;
        }
        let result = post(
            app,
            fixture.token,
            "/api/v1/playback/heartbeat",
            hint(
                play,
                Some(revision),
                2000,
                true,
                reason != "output_unsupported",
                "paused",
            ),
        )
        .await;
        if reason == "changed" {
            sqlx::query("UPDATE media_files SET file_size=file_size-1 WHERE id=$1")
                .bind(evidence.owned.file)
                .execute(fixture.pool)
                .await?;
        } else if reason == "unhealthy" {
            sqlx::query("UPDATE media_files SET is_healthy=true WHERE id=$1")
                .bind(evidence.owned.file)
                .execute(fixture.pool)
                .await?;
        }
        let response = result?;
        let expected = if reason == "output_unsupported" {
            reason
        } else {
            "source_changed"
        };
        anyhow::ensure!(
            response.0 == StatusCode::OK
                && response.1["recovery"]["status"] == "exhausted"
                && response.1["recovery"]["reason"] == expected
                && response.1["recovery"]["attempts"] == 0
                && response.1["transcode_session_id"].is_null()
                && state.transcode_manager.active_session_count() == 0,
            "direct refusal {reason} allocated output or reported the wrong outcome: {}",
            response.1
        );
        let stored = snapshot(fixture.pool, play).await?;
        anyhow::ensure!(stored.0["recovery_budget"]["exhausted"] == true);
        let repeated = post(
            app,
            fixture.token,
            "/api/v1/playback/heartbeat",
            hint(play, Some(revision), 2000, true, true, "paused"),
        )
        .await?;
        anyhow::ensure!(
            repeated.0 == StatusCode::OK
                && repeated.1["transcode_session_id"].is_null()
                && state.transcode_manager.active_session_count() == 0
        );
        stop(app, fixture.token, play).await?;
    }
    evidence.proof["capability_and_changed_unhealthy_refusal_without_workers"] = json!(true);
    Ok(())
}

async fn rollback_case(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    evidence: &mut DirectEvidence,
) -> anyhow::Result<()> {
    let start = start_direct(fixture, app, evidence.owned.item, evidence.owned.file).await?;
    let play = parse_uuid(&start, "session_id")?;
    let revision = parse_uuid(&start, "direct_source_revision")?;
    sqlx::query(r#"UPDATE play_sessions SET metadata=metadata || '{"direct_fixture_rollback":true}'::jsonb WHERE id=$1"#)
        .bind(play).execute(fixture.pool).await?;
    let before = snapshot(fixture.pool, play).await?;
    evidence.trigger = true;
    sqlx::query(r#"CREATE FUNCTION duskcue_direct_fixture_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.metadata @> '{"direct_fixture_rollback":true}'::jsonb AND NEW.metadata->>'transcode_session_id' IS NOT NULL THEN RAISE EXCEPTION 'owned direct fallback commit fixture'; END IF; RETURN NEW; END; $$"#)
        .execute(fixture.pool).await?;
    sqlx::query("CREATE TRIGGER duskcue_direct_fixture_failure BEFORE UPDATE ON play_sessions FOR EACH ROW EXECUTE FUNCTION duskcue_direct_fixture_failure()")
        .execute(fixture.pool).await?;
    let response = post(
        app,
        fixture.token,
        "/api/v1/playback/heartbeat",
        hint(play, Some(revision), 2000, true, true, "paused"),
    )
    .await?;
    anyhow::ensure!(
        response.0 == StatusCode::INTERNAL_SERVER_ERROR,
        "owned transaction failure did not reject promotion"
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while state.transcode_manager.active_session_count() != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    anyhow::ensure!(
        snapshot(fixture.pool, play).await? == before,
        "failed promotion transaction persisted a checkpoint, attempt or event"
    );
    drop_failure_trigger(fixture.pool, evidence.trigger).await?;
    evidence.trigger = false;
    stop(app, fixture.token, play).await?;
    evidence.proof["rollback_cancelled_unpublished_worker"] = json!(true);
    Ok(())
}

async fn drop_failure_trigger(pool: &sqlx::PgPool, installed: bool) -> anyhow::Result<()> {
    if installed {
        sqlx::query("DROP TRIGGER IF EXISTS duskcue_direct_fixture_failure ON play_sessions")
            .execute(pool)
            .await?;
        sqlx::query("DROP FUNCTION IF EXISTS duskcue_direct_fixture_failure()")
            .execute(pool)
            .await?;
    }
    Ok(())
}
