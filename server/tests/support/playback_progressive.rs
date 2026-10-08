// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::{Extension, Router};
use duskcue::domains::{playback, profiles};
use duskcue::services::encryption::EncryptionKey;
use duskcue::services::transcoding::TranscodeSession;
use duskcue::state::{AppState, RuntimeConfig};
use serde_json::{Value, json};
use uuid::Uuid;

#[path = "playback_progressive_media.rs"]
mod media;
#[path = "playback_progressive_process.rs"]
mod process;

use super::post;
use media::{fetch, playlist, segments};

pub(super) struct ProgressiveFixture<'a> {
    pub(super) pool: &'a sqlx::PgPool,
    pub(super) state: &'a AppState,
    pub(super) source: &'a Path,
    pub(super) token: &'a str,
    pub(super) peer_token: &'a str,
    pub(super) user: Uuid,
    pub(super) original_profile: Uuid,
    pub(super) kids_profile: Uuid,
    pub(super) library: Uuid,
}

pub(super) fn enabled() -> bool {
    std::env::var("DUSKCUE_TEST_PROGRESSIVE_PLAYBACK").as_deref() == Ok("event")
}

struct Session {
    play: Uuid,
    transcode: TranscodeSession,
    encoder: process::Encoder,
    segments: Vec<String>,
    timestamp: f64,
}

struct Stream<'a> {
    state: &'a AppState,
    app: &'a Router,
    token: &'a str,
    source: &'a Path,
    root: &'a Path,
    tools: &'a process::FixtureProcesses,
}

struct Evidence {
    root: PathBuf,
    item: Uuid,
    file: Uuid,
    sessions: Vec<Session>,
    source_seconds: Option<f64>,
    source_bytes: Option<u64>,
    first_segments: Option<usize>,
    seek_first_segments: Option<usize>,
    completed_segments: Option<usize>,
    profile_refused: bool,
    old_seek_released: bool,
    permit_reused: bool,
    tools: process::FixtureProcesses,
}

pub(super) async fn real_progressive_playback(
    fixture: ProgressiveFixture<'_>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        enabled()
            && std::env::var("DUSKCUE_TEST_FFMPEG_MODE").as_deref() == Ok("linux-local-managed"),
        "requires the explicit owned EVENT qualification mode"
    );
    let resource = Uuid::parse_str(&std::env::var("DUSKCUE_TEST_RESOURCE_ID")?)?;
    tokio::fs::create_dir_all(&fixture.state.bootstrap.data_dir).await?;
    let parent = fixture.state.bootstrap.data_dir.canonicalize()?;
    let root = parent.join(format!("progressive-http-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&root).await?;
    anyhow::ensure!(
        root.canonicalize()?.starts_with(&parent),
        "progressive fixture root escaped its owned parent"
    );
    let mut bootstrap = fixture.state.bootstrap.clone();
    bootstrap.data_dir = root.join("data");
    bootstrap.cache_dir = root.join("cache");
    let mut config = RuntimeConfig::default();
    config.cpu.ffmpeg_threads = Some(1);
    config.cpu.hw_accel_auto_detect = false;
    config.resource_limits.max_concurrent_transcodes = 1;
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
    let proof = exercise(&fixture, &state, &app, &mut evidence).await;
    let cleanup = cleanup(&fixture, &state, &app, &evidence).await;
    println!(
        "\nDUSKCUE_PROGRESSIVE_PLAYBACK={}",
        json!({
            "version": 1, "executed": true, "passed": proof.is_ok() && cleanup.is_ok(), "resource_id": resource,
            "source_seconds": evidence.source_seconds, "source_bytes": evidence.source_bytes,
            "first_segments": evidence.first_segments, "seek_first_segments": evidence.seek_first_segments, "completed_segments": evidence.completed_segments,
            "original_profile_binding_and_refusal": evidence.profile_refused, "seek_released_old_encoder_cache": evidence.old_seek_released,
            "stop_released_capacity": evidence.permit_reused, "cleanup_passed": cleanup.is_ok(),
            "fixture_children_confirmed_exited": evidence.tools.exited(), "encoder_exit_status": "unqualified_by_http_fixture",
            "encoder_pids": evidence.sessions.iter().map(|session| session.encoder.pid).collect::<Vec<_>>(),
            "encoder_identities": evidence.sessions.iter().map(|session| session.encoder.recorded_identity()).collect::<Vec<_>>(),
            "decoded_stream_relative_timestamps": evidence.sessions.iter().map(|session| session.timestamp).collect::<Vec<_>>(),
            "browser_to_live_server": "unqualified"
        })
    );
    proof?;
    cleanup
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
    sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','Progressive fixture','Progressive fixture','R')")
        .bind(evidence.item).bind(fixture.library).execute(fixture.pool).await?;
    sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,runtime_seconds,video_codec,video_resolution,video_bitrate,audio_codec,audio_channels,additional_streams) VALUES ($1,$2,$3,$4,'matroska',$5,'h264','320x180',400000,'aac',1,$6)")
        .bind(evidence.file).bind(evidence.item).bind(source.path.to_string_lossy().as_ref()).bind(i64::try_from(source.bytes)?)
        .bind(source.runtime_seconds).bind(source.audio).execute(fixture.pool).await?;
    tokio::time::timeout(Duration::from_secs(150), async {
        let stream = Stream {
            state,
            app,
            token: fixture.token,
            source: &source.path,
            root: &evidence.root,
            tools: &evidence.tools,
        };
        let first = start(&stream, evidence.item, evidence.file).await?;
        evidence.first_segments = Some(first.segments.len());
        let play = first.play;
        let first_id = first.transcode.id;
        let first_encoder = first.encoder.clone();
        let first_cache = first.transcode.segment_dir.clone();
        evidence.sessions.push(first);
        let capacity = post(
            app,
            fixture.token,
            "/api/v1/playback/start",
            start_body(evidence.item, evidence.file),
        )
        .await?;
        anyhow::ensure!(
            capacity.0 == StatusCode::SERVICE_UNAVAILABLE
                && state.transcode_manager.active_session_count() == 1,
            "a physically active encoder did not retain its one-slot permit"
        );
        first_encoder.alive()?;
        switch(app, fixture.token, fixture.kids_profile).await?;
        let forbidden = fetch(
            app,
            fixture.token,
            &format!("/api/v1/transcode/{first_id}/manifest.m3u8"),
        )
        .await?;
        anyhow::ensure!(
            forbidden.0 == StatusCode::NOT_FOUND,
            "new active profile accessed an original-profile stream"
        );
        let peer = fetch(
            app,
            fixture.peer_token,
            &format!("/api/v1/transcode/{first_id}/manifest.m3u8"),
        )
        .await?;
        anyhow::ensure!(
            peer.0 == StatusCode::NOT_FOUND,
            "another owner accessed the stream"
        );
        let (status, seek) = post(
            app,
            fixture.token,
            "/api/v1/playback/seek",
            json!({ "session_id": play, "position_ms": 30000 }),
        )
        .await?;
        anyhow::ensure!(
            status == StatusCode::OK && seek["position_ms"] == 30000,
            "real HTTP seek did not retain its absolute source target"
        );
        first_encoder.gone()?;
        anyhow::ensure!(
            !first_cache.exists() && state.transcode_manager.get_session(&first_id).is_none(),
            "seek did not release the old cache/session"
        );
        evidence.old_seek_released = true;
        let seek_id = parse_uuid(&seek, "transcode_session_id")?;
        anyhow::ensure!(
            seek_id != first_id
                && seek["session_id"] == play.to_string()
                && seek["stream_url"] == format!("/api/v1/transcode/{seek_id}/manifest.m3u8"),
            "HTTP seek did not publish its new stream generation"
        );
        let stored: (Uuid, Uuid) =
            sqlx::query_as("SELECT user_id,profile_id FROM play_sessions WHERE id=$1")
                .bind(play)
                .fetch_one(fixture.pool)
                .await?;
        anyhow::ensure!(
            stored == (fixture.user, fixture.original_profile),
            "seek rebound the playback to the new active profile"
        );
        let forbidden = fetch(
            app,
            fixture.token,
            &format!("/api/v1/transcode/{seek_id}/manifest.m3u8"),
        )
        .await?;
        anyhow::ensure!(
            forbidden.0 == StatusCode::NOT_FOUND,
            "new active profile accessed the replacement stream"
        );
        switch(app, fixture.token, fixture.original_profile).await?;
        let second = observe(&stream, play, seek_id, Some(30000)).await?;
        evidence.seek_first_segments = Some(second.segments.len());
        let second_encoder = second.encoder.clone();
        let second_cache = second.transcode.segment_dir.clone();
        let mut previous = second.segments.clone();
        evidence.sessions.push(second);
        let (status, info) = super::request(
            app,
            fixture.token,
            "GET",
            &format!("/api/v1/playback/info/{play}"),
            json!({}),
        )
        .await?;
        anyhow::ensure!(
            status == StatusCode::OK
                && info["duration_ms"] == media::SOURCE_SECONDS * 1000
                && info["position_ms"] == 30000,
            "HTTP playback info confused growing stream duration with source runtime/offset"
        );
        let kids_rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM user_item_data WHERE profile_id=$1 AND media_item_id=$2",
        )
        .bind(fixture.kids_profile)
        .bind(evidence.item)
        .fetch_one(fixture.pool)
        .await?;
        anyhow::ensure!(
            kids_rows == 0,
            "seek history was written to the new active profile"
        );
        evidence.profile_refused = true;
        let mut grew = false;
        loop {
            media::cache_bytes(&evidence.root)?;
            let (status, playlist) = playlist(app, fixture.token, seek_id).await?;
            anyhow::ensure!(
                status == StatusCode::OK
                    && playlist
                        .lines()
                        .any(|line| line == "#EXT-X-PLAYLIST-TYPE:EVENT"),
                "growing HTTP playlist lost its EVENT publication contract"
            );
            let segments = segments(&playlist)?;
            anyhow::ensure!(
                segments.starts_with(&previous),
                "EVENT playlist removed or changed published segments"
            );
            grew |= segments.len() > previous.len();
            previous = segments;
            if playlist.lines().any(|line| line == "#EXT-X-ENDLIST") {
                anyhow::ensure!(
                    grew,
                    "real progressive playlist never grew after HTTP readiness"
                );
                evidence.completed_segments = Some(previous.len());
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while second_encoder.gone().is_err() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;
        stop(app, fixture.token, play).await?;
        anyhow::ensure!(
            !second_cache.exists() && state.transcode_manager.get_session(&seek_id).is_none(),
            "completed HTTP stop did not release cache/session"
        );
        let third = start(&stream, evidence.item, evidence.file).await?;
        let third_play = third.play;
        let third_id = third.transcode.id;
        let third_encoder = third.encoder.clone();
        let third_cache = third.transcode.segment_dir.clone();
        evidence.sessions.push(third);
        third_encoder.alive()?;
        stop(app, fixture.token, third_play).await?;
        third_encoder.gone()?;
        anyhow::ensure!(
            !third_cache.exists() && state.transcode_manager.get_session(&third_id).is_none(),
            "active HTTP stop returned before cache/session release"
        );
        let fourth = start(&stream, evidence.item, evidence.file).await?;
        let fourth_play = fourth.play;
        evidence.sessions.push(fourth);
        evidence.permit_reused = true;
        stop(app, fixture.token, fourth_play).await?;
        media::cache_bytes(&evidence.root)?;
        anyhow::ensure!(
            state.transcode_manager.active_session_count() == 0,
            "progressive manager retains a session after awaited stop"
        );
        Ok::<_, anyhow::Error>(())
    })
    .await??;
    Ok(())
}

fn start_body(item: Uuid, file: Uuid) -> Value {
    json!({ "media_item_id": item, "media_file_id": file, "audio_stream_index": 2, "force_transcode": true, "playback_mode": "interactive" })
}

async fn start(stream: &Stream<'_>, item: Uuid, file: Uuid) -> anyhow::Result<Session> {
    let (status, body) = post(
        stream.app,
        stream.token,
        "/api/v1/playback/start",
        start_body(item, file),
    )
    .await?;
    anyhow::ensure!(
        status == StatusCode::OK
            && body["stream_decision"] == "transcode"
            && body["selected_audio_stream_index"] == 2,
        "real HTTP start did not launch the selected production encoder"
    );
    let play = parse_uuid(&body, "session_id")?;
    let transcode = parse_uuid(&body, "transcode_session_id")?;
    anyhow::ensure!(
        body["stream_url"] == format!("/api/v1/transcode/{transcode}/manifest.m3u8"),
        "HTTP start stream URL differs from the actual session"
    );
    observe(stream, play, transcode, None).await
}

async fn observe(
    stream: &Stream<'_>,
    play: Uuid,
    id: Uuid,
    seek_ms: Option<i64>,
) -> anyhow::Result<Session> {
    let transcode = stream
        .state
        .transcode_manager
        .get_session(&id)
        .ok_or_else(|| anyhow::anyhow!("actual manager session unavailable"))?;
    anyhow::ensure!(
        transcode.source_path == stream.source
            && transcode
                .segment_dir
                .canonicalize()?
                .starts_with(stream.root),
        "manager source/cache escaped the fixture generation"
    );
    let encoder = process::Encoder::observe(stream.source, &transcode.manifest_path, seek_ms)?;
    encoder.alive()?;
    let (status, playlist) = playlist(stream.app, stream.token, id).await?;
    anyhow::ensure!(
        status == StatusCode::OK
            && playlist
                .lines()
                .any(|line| line == "#EXT-X-PLAYLIST-TYPE:EVENT")
            && !playlist.lines().any(|line| line == "#EXT-X-ENDLIST"),
        "HTTP readiness requires playable EVENT output before real encoder completion"
    );
    let segments = segments(&playlist)?;
    let init = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|uri| uri.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("EVENT initialization URI unavailable"))?;
    let prefix = format!("/api/v1/transcode/{id}/{}/", transcode.rendition_name);
    anyhow::ensure!(
        init == format!("{prefix}init.mp4") && segments.iter().all(|uri| uri.starts_with(&prefix)),
        "HTTP manifest references another stream generation"
    );
    let (status, initialization) = fetch(stream.app, stream.token, init).await?;
    anyhow::ensure!(
        status == StatusCode::OK,
        "real HTTP initialization unavailable"
    );
    let (status, fragment) = fetch(stream.app, stream.token, &segments[0]).await?;
    anyhow::ensure!(
        status == StatusCode::OK,
        "real HTTP first segment unavailable"
    );
    encoder.alive()?;
    let timestamp = media::decode_start(
        stream.tools,
        stream.root,
        &format!("http-{id}"),
        &initialization,
        &fragment,
    )
    .await?;
    encoder.alive()?;
    Ok(Session {
        play,
        transcode,
        encoder,
        segments,
        timestamp,
    })
}

fn parse_uuid(body: &Value, key: &str) -> anyhow::Result<Uuid> {
    Ok(Uuid::parse_str(body[key].as_str().ok_or_else(|| {
        anyhow::anyhow!("HTTP session identity unavailable")
    })?)?)
}

async fn switch(app: &Router, token: &str, profile: Uuid) -> anyhow::Result<()> {
    anyhow::ensure!(
        post(
            app,
            token,
            &format!("/api/v1/profiles/{profile}/switch"),
            json!({})
        )
        .await?
        .0 == StatusCode::OK,
        "fixture profile switch failed"
    );
    Ok(())
}

async fn stop(app: &Router, token: &str, play: Uuid) -> anyhow::Result<()> {
    anyhow::ensure!(
        post(
            app,
            token,
            "/api/v1/playback/stop",
            json!({ "session_id": play, "position_ms": 30000 })
        )
        .await?
        .0 == StatusCode::OK,
        "awaited real HTTP stop failed"
    );
    Ok(())
}

async fn cleanup(
    fixture: &ProgressiveFixture<'_>,
    state: &AppState,
    app: &Router,
    evidence: &Evidence,
) -> anyhow::Result<()> {
    let mut errors = Vec::new();
    let mut physically_released = evidence.tools.exited();
    if !physically_released {
        errors.push(
            "original fixture child exit is unconfirmed; root retained for owned-container cleanup"
                .into(),
        );
    }
    let plays = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM play_sessions WHERE media_item_id=$1 AND user_id=$2",
    )
    .bind(evidence.item)
    .bind(fixture.user)
    .fetch_all(fixture.pool)
    .await;
    match plays {
        Ok(plays) => {
            for play in plays {
                match tokio::time::timeout(Duration::from_secs(10), stop(app, fixture.token, play))
                    .await
                {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => errors.push(error.to_string()),
                    Err(error) => errors.push(error.to_string()),
                }
            }
        }
        Err(error) => errors.push(error.to_string()),
    }
    for session in state.transcode_manager.list_active_sessions() {
        if session.segment_dir.starts_with(&evidence.root) {
            match tokio::time::timeout(
                Duration::from_secs(10),
                state.transcode_manager.stop_session(session.id),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => errors.push(error.to_string()),
                Err(error) => errors.push(error.to_string()),
            }
        } else {
            errors.push("fixture manager contains an unowned session".into());
        }
    }
    for session in &evidence.sessions {
        if let Err(error) = session.encoder.gone() {
            physically_released = false;
            errors.push(error.to_string());
        }
    }
    if let Err(error) = switch(app, fixture.token, fixture.original_profile).await {
        errors.push(error.to_string());
    }
    match sqlx::query_scalar::<_, i64>(
        "SELECT count(*) FROM play_sessions WHERE media_item_id=$1 AND stopped_at IS NULL",
    )
    .bind(evidence.item)
    .fetch_one(fixture.pool)
    .await
    {
        Ok(0) => {}
        Ok(_) => errors.push("progressive playback row is still active".into()),
        Err(error) => errors.push(error.to_string()),
    }
    if state.transcode_manager.active_session_count() != 0 {
        physically_released = false;
        errors.push("owned manager retains a session after cleanup".into());
    }
    let parent = fixture.state.bootstrap.data_dir.canonicalize()?;
    anyhow::ensure!(
        evidence.root.canonicalize()? == evidence.root
            && evidence.root.parent() == Some(parent.as_path())
            && evidence
                .root
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("progressive-http-")),
        "cleanup root is outside its exact owned fixture"
    );
    if physically_released {
        if let Err(error) = sqlx::query("DELETE FROM media_items WHERE id=$1 AND library_id=$2")
            .bind(evidence.item)
            .bind(fixture.library)
            .execute(fixture.pool)
            .await
        {
            errors.push(error.to_string());
        }
        if let Err(error) = tokio::fs::remove_dir_all(&evidence.root).await {
            errors.push(error.to_string());
        }
    }
    anyhow::ensure!(
        errors.is_empty(),
        "progressive fixture cleanup failed: {}",
        errors.join("; ")
    );
    Ok(())
}
