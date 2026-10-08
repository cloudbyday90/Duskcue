// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use duskcue::config::BootstrapConfig;
use duskcue::domains::{playback, profiles};
use duskcue::services::encryption::EncryptionKey;
use duskcue::state::AppState;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::path::{Path, PathBuf};
use tower::ServiceExt;

#[path = "support/guarded_docker.rs"]
mod guarded_docker;

#[path = "support/playback_stop_profiles.rs"]
mod playback_stop_profiles;
use playback_stop_profiles::{DeletedProfileFixture, deleted_profile_cleanup};
#[cfg(target_os = "linux")]
#[path = "support/playback_progressive.rs"]
mod playback_progressive;
use uuid::Uuid;

async fn post(
    app: &Router,
    token: &str,
    path: &str,
    body: Value,
) -> anyhow::Result<(StatusCode, Value)> {
    request(app, token, "POST", path, body).await
}

async fn request(
    app: &Router,
    token: &str,
    method: &str,
    path: &str,
    body: Value,
) -> anyhow::Result<(StatusCode, Value)> {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body)?))?,
        )
        .await?;
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 65536).await?;
    let problem: Value = serde_json::from_slice(&bytes)?;
    if status.is_server_error() {
        eprintln!(
            "DUSKCUE_STOP_CONTRACT_PROBLEM={}",
            json!({
                "method": method,
                "path": path,
                "status": status.as_u16(),
                "title": problem.get("title"),
                "detail": problem.get("detail"),
            })
        );
    }
    Ok((status, problem))
}

fn source_fixture() -> anyhow::Result<PathBuf> {
    let source = PathBuf::from(std::env::var("DUSKCUE_TEST_PLAYBACK_SOURCE")?).canonicalize()?;
    let cache = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".cache/tonight-server-ffmpeg")
        .canonicalize()?;
    anyhow::ensure!(
        source.starts_with(cache)
            && source.file_name().and_then(|value| value.to_str()) == Some("source.mkv"),
        "requires the actual ignored eight-second FFmpeg source fixture"
    );
    Ok(source)
}

async fn wait_for_lock(pool: &sqlx::PgPool, count: i64) -> anyhow::Result<()> {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let waiting: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND application_name='duskcue_stop_contract' AND wait_event_type='Lock'")
                .fetch_one(pool).await?;
            if waiting >= count { return Ok::<_, sqlx::Error>(()); }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await??;
    Ok(())
}

async fn insert_play(
    pool: &sqlx::PgPool,
    user: Uuid,
    profile: Uuid,
    item: Uuid,
    library: Uuid,
    file: Uuid,
) -> anyhow::Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO play_sessions(id,user_id,profile_id,media_item_id,library_id,started_at,client_name,stream_decision,metadata) VALUES ($1,$2,$3,$4,$5,now()-interval '30 seconds','stop fixture','direct_play',$6)")
        .bind(id).bind(user).bind(profile).bind(item).bind(library)
        .bind(json!({"media_file_id": file, "current_position_ms": 0, "current_state": "playing", "fixture_sibling": {"preserved": true}})).execute(pool).await?;
    Ok(id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires guarded disposable PostgreSQL18 and DUSKCUE_TRANSCODE_ACCESS_TESTS=disposable"]
async fn stop_is_idempotent_serializes_heartbeat_seek_and_retains_original_profile()
-> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS").as_deref() == Ok("disposable"),
        "requires disposable opt-in"
    );
    let source = source_fixture()?;
    let (container, database_url) = if std::env::var("DUSKCUE_DATABASE_URL").is_ok() {
        (None, std::env::var("DUSKCUE_DATABASE_URL")?)
    } else {
        let (container, url) = postgres().await?;
        (Some(container), url)
    };
    let pool = PgPoolOptions::new()
        .max_connections(8)
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query("SET application_name='duskcue_stop_contract'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_transcode_test"),
        "requires an explicitly disposable transcode fixture database"
    );
    sqlx::migrate!().run(&pool).await?;
    sqlx::query("CREATE TABLE IF NOT EXISTS play_sessions_transcode_test PARTITION OF play_sessions DEFAULT").execute(&pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS play_events_transcode_test PARTITION OF play_events DEFAULT",
    )
    .execute(&pool)
    .await?;
    let owner = Uuid::now_v7();
    let peer = Uuid::now_v7();
    let original = Uuid::now_v7();
    let kids = Uuid::now_v7();
    let peer_profile = Uuid::now_v7();
    for id in [owner, peer] {
        sqlx::query("INSERT INTO users(id,username,display_name,has_all_library_access) VALUES ($1,$2,'Stop contract',true)")
            .bind(id).bind(format!("stop-{id}")).execute(&pool).await?;
    }
    for (id, user, is_default) in [
        (original, owner, true),
        (kids, owner, false),
        (peer_profile, peer, true),
    ] {
        sqlx::query(
            "INSERT INTO user_profiles(id,owner_user_id,name,is_default) VALUES ($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(user)
        .bind(format!("Stop profile {id}"))
        .bind(is_default)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "UPDATE user_profiles SET profile_type='kids',max_content_rating='TV-PG' WHERE id=$1",
    )
    .bind(kids)
    .execute(&pool)
    .await?;
    let library = Uuid::now_v7();
    let item = Uuid::now_v7();
    let file = Uuid::now_v7();
    sqlx::query("INSERT INTO libraries(id,name,slug,media_type,root_path) VALUES ($1,'Stop library',$2,'movies','fixture')").bind(library).bind(format!("stop-{library}")).execute(&pool).await?;
    sqlx::query("INSERT INTO media_items(id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','Stop movie','Stop movie','R')").bind(item).bind(library).execute(&pool).await?;
    sqlx::query("INSERT INTO media_files(id,media_item_id,file_path,file_size,container_format,runtime_seconds) VALUES ($1,$2,$3,1,'mp4',20)")
        .bind(file).bind(item).bind(format!("stop-{file}.mp4")).execute(&pool).await?;
    let mut tokens = Vec::new();
    for (user, profile) in [(owner, original), (peer, peer_profile)] {
        let token = duskcue::domains::auth::service::generate_session_token();
        let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        sqlx::query("INSERT INTO user_sessions(id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,now()+interval '1 hour')")
            .bind(Uuid::now_v7()).bind(user).bind(profile).bind(hash).execute(&pool).await?;
        tokens.push(token);
    }
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(format!(".cache/tonight-server-stop/{owner}/data")),
        cache_dir: Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(format!(".cache/tonight-server-stop/{owner}/cache")),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), key);
    let app = playback::router(state.clone())
        .merge(profiles::router(state.clone()))
        .with_state(state.clone());
    let session = insert_play(&pool, owner, original, item, library, file).await?;
    assert_eq!(
        post(
            &app,
            &tokens[0],
            &format!("/api/v1/profiles/{kids}/switch"),
            json!({})
        )
        .await?
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(
            &app,
            &tokens[0],
            "/api/v1/playback/heartbeat",
            json!({"session_id":session,"position_ms":5000,"state":"paused"})
        )
        .await?
        .0,
        StatusCode::OK
    );
    assert_eq!(
        post(
            &app,
            &tokens[1],
            "/api/v1/playback/stop",
            json!({"session_id":session,"position_ms":19000})
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );
    let body = json!({"session_id":session,"position_ms":19000});
    let (first, second) = tokio::join!(
        post(&app, &tokens[0], "/api/v1/playback/stop", body.clone()),
        post(&app, &tokens[0], "/api/v1/playback/stop", body)
    );
    let first = first?;
    let second = second?;
    assert_eq!(first.0, StatusCode::OK);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(first.1, second.1);
    assert_eq!(first.1["is_watched"], json!(true));
    assert_eq!(first.1["play_count"], json!(1));
    let replay = post(
        &app,
        &tokens[0],
        "/api/v1/playback/stop",
        json!({"session_id":session,"position_ms":1}),
    )
    .await?;
    assert_eq!(replay, first);
    let watched:(bool,i32,i32)=sqlx::query_as("SELECT is_watched,play_count,resume_position_ms FROM user_item_data WHERE profile_id=$1 AND media_item_id=$2")
        .bind(original).bind(item).fetch_one(&pool).await?;
    assert_eq!(watched, (true, 1, 0));
    let kids_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM user_item_data WHERE profile_id=$1")
            .bind(kids)
            .fetch_one(&pool)
            .await?;
    assert_eq!(kids_rows, 0);
    let events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM play_events WHERE play_session_id=$1 AND event_type='stop'",
    )
    .bind(session)
    .fetch_one(&pool)
    .await?;
    assert_eq!(events, 1);
    let metadata: Value = sqlx::query_scalar("SELECT metadata FROM play_sessions WHERE id=$1")
        .bind(session)
        .fetch_one(&pool)
        .await?;
    assert_eq!(metadata["fixture_sibling"]["preserved"], json!(true));
    assert!(metadata.get("duskcue_stop_result_v1").is_some());
    assert_eq!(
        post(
            &app,
            &tokens[0],
            "/api/v1/playback/heartbeat",
            json!({"session_id":session,"position_ms":1000})
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        post(
            &app,
            &tokens[0],
            "/api/v1/playback/seek",
            json!({"session_id":session,"position_ms":1000})
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );

    for pending_path in ["/api/v1/playback/heartbeat", "/api/v1/playback/seek"] {
        let session = insert_play(&pool, owner, original, item, library, file).await?;
        let mut blocker = pool.begin().await?;
        sqlx::query("SELECT id FROM play_sessions WHERE id=$1 FOR UPDATE")
            .bind(session)
            .fetch_one(&mut *blocker)
            .await?;
        let stop_app = app.clone();
        let stop_token = tokens[0].clone();
        let stop = tokio::spawn(async move {
            post(
                &stop_app,
                &stop_token,
                "/api/v1/playback/stop",
                json!({"session_id":session,"position_ms":19000}),
            )
            .await
        });
        wait_for_lock(&pool, 1).await?;
        let pending_app = app.clone();
        let pending_token = tokens[0].clone();
        let pending = tokio::spawn(async move {
            post(
                &pending_app,
                &pending_token,
                pending_path,
                json!({"session_id":session,"position_ms":1000}),
            )
            .await
        });
        wait_for_lock(&pool, 2).await?;
        blocker.commit().await?;
        assert_eq!(stop.await??.0, StatusCode::OK);
        assert_eq!(pending.await??.0, StatusCode::NOT_FOUND);
        let final_state:(bool,i32,String)=sqlx::query_as("SELECT uid.is_watched,uid.resume_position_ms,ps.metadata ->> 'current_state' FROM play_sessions ps JOIN user_item_data uid ON uid.profile_id=ps.profile_id AND uid.media_item_id=ps.media_item_id WHERE ps.id=$1")
            .bind(session).fetch_one(&pool).await?;
        assert_eq!(final_state, (true, 0, "stopped".into()));
    }
    let legacy = insert_play(&pool, owner, original, item, library, file).await?;
    sqlx::query("UPDATE play_sessions SET stopped_at=now(),duration_seconds=10,percent_complete=100 WHERE id=$1").bind(legacy).execute(&pool).await?;
    let before: i32 = sqlx::query_scalar(
        "SELECT play_count FROM user_item_data WHERE profile_id=$1 AND media_item_id=$2",
    )
    .bind(original)
    .bind(item)
    .fetch_one(&pool)
    .await?;
    let stopped = post(
        &app,
        &tokens[0],
        "/api/v1/playback/stop",
        json!({"session_id":legacy,"position_ms":1}),
    )
    .await?;
    assert_eq!(stopped.0, StatusCode::OK);
    assert_eq!(stopped.1["play_count"], json!(before));
    let after: i32 = sqlx::query_scalar(
        "SELECT play_count FROM user_item_data WHERE profile_id=$1 AND media_item_id=$2",
    )
    .bind(original)
    .bind(item)
    .fetch_one(&pool)
    .await?;
    assert_eq!(before, after);
    assert_eq!(
        post(
            &app,
            &tokens[0],
            &format!("/api/v1/profiles/{original}/switch"),
            json!({})
        )
        .await?
        .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE media_files SET file_path=$2,container_format='matroska',runtime_seconds=8 WHERE id=$1")
        .bind(file).bind(source.to_string_lossy().as_ref()).execute(&pool).await?;
    deleted_profile_cleanup(DeletedProfileFixture {
        app: &app,
        pool: &pool,
        state: &state,
        token: &tokens[0],
        peer_token: &tokens[1],
        user: owner,
        item,
        library,
        file,
        source: &source,
    })
    .await?;
    #[cfg(target_os = "linux")]
    if playback_progressive::enabled() {
        playback_progressive::real_progressive_playback(playback_progressive::ProgressiveFixture {
            pool: &pool,
            state: &state,
            source: &source,
            token: &tokens[0],
            peer_token: &tokens[1],
            user: owner,
            original_profile: original,
            kids_profile: kids,
            library,
        })
        .await?;
    }
    let deleted_file_session = insert_play(&pool, owner, original, item, library, file).await?;
    sqlx::query("DELETE FROM media_files WHERE id=$1")
        .bind(file)
        .execute(&pool)
        .await?;
    assert_eq!(
        post(
            &app,
            &tokens[0],
            "/api/v1/playback/heartbeat",
            json!({"session_id":deleted_file_session,"position_ms":5000})
        )
        .await?
        .0,
        StatusCode::OK
    );
    let stopped = post(
        &app,
        &tokens[0],
        "/api/v1/playback/stop",
        json!({"session_id":deleted_file_session,"position_ms":19000}),
    )
    .await?;
    assert_eq!(stopped.0, StatusCode::OK);
    assert!(stopped.1["percent_complete"].is_null());
    assert_eq!(
        post(
            &app,
            &tokens[0],
            "/api/v1/playback/stop",
            json!({"session_id":deleted_file_session,"position_ms":1})
        )
        .await?,
        stopped
    );
    let retained_file:Option<Uuid>=sqlx::query_scalar("SELECT last_played_media_file_id FROM user_item_data WHERE profile_id=$1 AND media_item_id=$2")
        .bind(original).bind(item).fetch_one(&pool).await?;
    assert!(retained_file.is_none());
    sqlx::query("DELETE FROM users WHERE id=ANY($1)")
        .bind(vec![owner, peer])
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM libraries WHERE id=$1")
        .bind(library)
        .execute(&pool)
        .await?;
    pool.close().await;
    if let Some(container) = container {
        container.remove().await?;
    }
    Ok(())
}

async fn postgres() -> anyhow::Result<(guarded_docker::GuardedContainer, String)> {
    let fixture = guarded_docker::GuardedContainer::registered()?;
    let mut args = fixture.run_args();
    args.extend([
        "--detach".into(),
        "--publish".into(),
        "127.0.0.1::5432".into(),
        "--env".into(),
        "POSTGRES_DB=duskcue_transcode_test_stop".into(),
        "--env".into(),
        "POSTGRES_USER=duskcue_fixture".into(),
        "--env".into(),
        "POSTGRES_PASSWORD=disposable-stop-fixture".into(),
        "postgres:18".into(),
    ]);
    anyhow::ensure!(
        guarded_docker::command(&args).await?.status.success(),
        "disposable PostgreSQL could not start"
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let ready = guarded_docker::command(&[
            "exec".into(),
            fixture.name.clone(),
            "pg_isready".into(),
            "-U".into(),
            "duskcue_fixture".into(),
            "-d".into(),
            "duskcue_transcode_test_stop".into(),
        ])
        .await?;
        if ready.status.success() {
            break;
        }
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "disposable PostgreSQL did not become ready"
        );
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let port =
        guarded_docker::command(&["port".into(), fixture.name.clone(), "5432/tcp".into()]).await?;
    anyhow::ensure!(port.status.success(), "fixture PostgreSQL port unavailable");
    let address = String::from_utf8(port.stdout)?;
    let number = address
        .trim()
        .strip_prefix("127.0.0.1:")
        .ok_or_else(|| anyhow::anyhow!("fixture database is not on loopback"))?
        .parse::<u16>()?;
    Ok((
        fixture,
        format!(
            "postgres://duskcue_fixture:disposable-stop-fixture@127.0.0.1:{number}/duskcue_transcode_test_stop"
        ),
    ))
}
