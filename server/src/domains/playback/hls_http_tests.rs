// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Request, StatusCode};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

use crate::config::BootstrapConfig;
use crate::domains::playback;
use crate::services::encryption::EncryptionKey;
use crate::services::transcoding::{HwAccelMethod, TranscodeSession};
use crate::state::AppState;

async fn get(
    app: &Router,
    token: Option<&str>,
    path: &str,
) -> anyhow::Result<(StatusCode, HeaderMap, Vec<u8>)> {
    let mut request = Request::builder().uri(path);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = app.clone().oneshot(request.body(Body::empty())?).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1_000_000).await?.to_vec();
    Ok((status, headers, bytes))
}

async fn copy_media(source: &Path, target: &Path, playlist: &str) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(target).await?;
    let files = [playlist.to_string(), "init.mp4".into()]
        .into_iter()
        .chain((0..5).map(|index| format!("seg_{index:04}.m4s")));
    for file in files {
        tokio::fs::copy(source.join(&file), target.join(&file)).await?;
    }
    Ok(())
}

fn session(id: Uuid, owner: Uuid, directory: PathBuf, rendition: &str) -> TranscodeSession {
    TranscodeSession {
        id,
        user_id: owner,
        media_file_id: Uuid::now_v7(),
        started_at: Utc::now(),
        source_path: PathBuf::from("fixture.mp4"),
        source_video_codec: "h264".into(),
        source_video_resolution: (320, 180),
        source_audio_codec: "aac".into(),
        source_audio_stream_index: Some(1),
        subtitle_stream_index: None,
        subtitle_stream_ordinal: None,
        subtitle_burn_in: false,
        target_video_codec: "h264".into(),
        target_video_resolution: (320, 180),
        target_audio_codec: "aac".into(),
        target_bitrate: 400_000,
        hw_accel: HwAccelMethod::Software,
        rendition_name: rendition.into(),
        manifest_path: directory.join("manifest.m3u8"),
        segment_dir: directory,
        segments_written: 5,
        client_position_segment: 0,
        progress: None,
        is_complete: true,
        is_seeking: false,
    }
}

#[tokio::test]
#[ignore = "requires disposable PostgreSQL18, DUSKCUE_TRANSCODE_ACCESS_TESTS=disposable and DUSKCUE_HLS_TEST_MEDIA_DIR containing real FFmpeg fixtures"]
async fn real_fmp4_http_paths_preserve_auth_policy_and_reject_traversal() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS").as_deref() == Ok("disposable"),
        "requires disposable fixture opt-in"
    );
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_transcode_test"),
        "requires a duskcue_transcode_test disposable database"
    );
    sqlx::migrate!().run(&pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS play_sessions_transcode_test PARTITION OF play_sessions DEFAULT",
    )
    .execute(&pool)
    .await?;
    let fixtures = PathBuf::from(std::env::var("DUSKCUE_HLS_TEST_MEDIA_DIR")?);
    let temporary = std::env::temp_dir().join(format!("duskcue-hls-http-{}", Uuid::now_v7()));
    let single = temporary.join("single");
    let master = temporary.join("master");
    copy_media(&fixtures.join("single"), &single, "manifest.m3u8").await?;
    copy_media(
        &fixtures.join("master/720p"),
        &master.join("720p"),
        "index.m3u8",
    )
    .await?;
    tokio::fs::copy(
        fixtures.join("master/manifest.m3u8"),
        master.join("manifest.m3u8"),
    )
    .await?;

    let owner = Uuid::now_v7();
    let peer = Uuid::now_v7();
    let profile = Uuid::now_v7();
    let other_profile = Uuid::now_v7();
    let peer_profile = Uuid::now_v7();
    for id in [owner, peer] {
        sqlx::query("INSERT INTO users (id,username,display_name,has_all_library_access) VALUES ($1,$2,'HLS fixture',true)")
            .bind(id).bind(format!("hls-{id}")).execute(&pool).await?;
    }
    for (id, user) in [
        (profile, owner),
        (other_profile, owner),
        (peer_profile, peer),
    ] {
        sqlx::query("INSERT INTO user_profiles (id,owner_user_id,name,is_default) VALUES ($1,$2,'HLS fixture',$3)")
            .bind(id).bind(user).bind(id!=other_profile).execute(&pool).await?;
    }
    let library = Uuid::now_v7();
    let item = Uuid::now_v7();
    sqlx::query("INSERT INTO libraries (id,name,slug,media_type,root_path) VALUES ($1,'HLS fixture',$2,'movies','fixture')")
        .bind(library).bind(format!("hls-{library}")).execute(&pool).await?;
    sqlx::query("INSERT INTO media_items (id,library_id,type,title,sort_title,content_rating) VALUES ($1,$2,'movie','HLS','HLS','PG')")
        .bind(item).bind(library).execute(&pool).await?;
    let transcodes = [Uuid::now_v7(), Uuid::now_v7()];
    for id in transcodes {
        sqlx::query("INSERT INTO play_sessions (id,user_id,profile_id,media_item_id,library_id,started_at,client_name,stream_decision,metadata) VALUES ($1,$2,$3,$4,$5,now(),'fixture','transcode',$6)")
            .bind(Uuid::now_v7()).bind(owner).bind(profile).bind(item).bind(library).bind(json!({"transcode_session_id":id})).execute(&pool).await?;
    }
    let mut tokens = Vec::new();
    for (user_id, profile_id) in [
        (owner, profile),
        (owner, other_profile),
        (peer, peer_profile),
    ] {
        let token = crate::domains::auth::service::generate_session_token();
        let hash = ring::digest::digest(&ring::digest::SHA256, token.as_bytes())
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        sqlx::query("INSERT INTO user_sessions (id,user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,$4,now()+interval '1 hour')")
            .bind(Uuid::now_v7()).bind(user_id).bind(profile_id).bind(hash).execute(&pool).await?;
        tokens.push(token);
    }
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: temporary.clone(),
        cache_dir: temporary.join("cache"),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (encryption_key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), encryption_key);
    state.transcode_manager.insert_test_session(session(
        transcodes[0],
        owner,
        single.clone(),
        "remux",
    ));
    state.transcode_manager.insert_test_session(session(
        transcodes[1],
        owner,
        master.clone(),
        "auto",
    ));
    let app = playback::router(state.clone()).with_state(state);

    let mut accessible_paths = Vec::new();
    for (id, rendition, directory, is_master) in [
        (transcodes[0], "remux", single, false),
        (transcodes[1], "720p", master.join("720p"), true),
    ] {
        let manifest_url = format!("/api/v1/transcode/{id}/manifest.m3u8");
        let playlist_url = format!("/api/v1/transcode/{id}/{rendition}/index.m3u8");
        let (status, headers, bytes) = get(&app, Some(&tokens[0]), &manifest_url).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get("content-type").unwrap(),
            "application/vnd.apple.mpegurl"
        );
        assert!(
            headers
                .get("cache-control")
                .unwrap()
                .to_str()?
                .contains("no-store")
        );
        let content = String::from_utf8(bytes)?;
        if is_master {
            assert!(content.contains(&playlist_url));
        } else {
            assert!(content.contains(&format!("/{rendition}/init.mp4")));
        }
        let (status, headers, bytes) = get(&app, Some(&tokens[0]), &playlist_url).await?;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            headers.get("content-type").unwrap(),
            "application/vnd.apple.mpegurl"
        );
        assert!(
            headers
                .get("cache-control")
                .unwrap()
                .to_str()?
                .contains("no-store")
        );
        assert!(
            String::from_utf8(bytes)?
                .contains(&format!("/api/v1/transcode/{id}/{rendition}/seg_0000.m4s"))
        );
        accessible_paths.extend([manifest_url, playlist_url]);
        let files = [("init.mp4".to_string(), "video/mp4")]
            .into_iter()
            .chain((0..5).map(|index| (format!("seg_{index:04}.m4s"), "video/iso.segment")));
        for (file, content_type) in files {
            let path = format!("/api/v1/transcode/{id}/{rendition}/{file}");
            let (status, headers, bytes) = get(&app, Some(&tokens[0]), &path).await?;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(headers.get("content-type").unwrap(), content_type);
            assert_eq!(headers.get("cache-control").unwrap(), "private, no-store");
            assert_eq!(bytes, tokio::fs::read(directory.join(&file)).await?);
            assert_eq!(
                headers
                    .get("content-length")
                    .unwrap()
                    .to_str()?
                    .parse::<usize>()?,
                bytes.len()
            );
            if file == "init.mp4" {
                assert_eq!(&bytes[4..8], b"ftyp");
            }
            accessible_paths.push(path);
        }
        for tail in [
            "unknown/init.mp4",
            "../init.mp4",
            "remux/..%2finit.mp4",
            "remux/seg_0000.m4s%2f..%2fsecret",
            "remux/seg_secrets.m4s",
            "remux/manifest.m3u8",
        ] {
            let path = format!("/api/v1/transcode/{id}/{tail}");
            assert_eq!(
                get(&app, Some(&tokens[0]), &path).await?.0,
                StatusCode::NOT_FOUND,
                "{path}"
            );
        }
    }
    let master_manifest = master.join("manifest.m3u8");
    let original_master = tokio::fs::read(&master_manifest).await?;
    tokio::fs::write(
        &master_manifest,
        "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1\nmissing/index.m3u8\n",
    )
    .await?;
    assert_eq!(
        get(
            &app,
            Some(&tokens[0]),
            &format!("/api/v1/transcode/{}/manifest.m3u8", transcodes[1])
        )
        .await?
        .0,
        StatusCode::NOT_FOUND
    );
    tokio::fs::write(master_manifest, original_master).await?;
    for path in &accessible_paths {
        assert_eq!(get(&app, None, path).await?.0, StatusCode::UNAUTHORIZED);
        for token in &tokens[1..] {
            assert_eq!(get(&app, Some(token), path).await?.0, StatusCode::NOT_FOUND);
        }
    }
    sqlx::query(
        "UPDATE user_profiles SET profile_type='kids',max_content_rating='TV-PG' WHERE id=$1",
    )
    .bind(profile)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO profile_library_access (profile_id,library_id) VALUES ($1,$2)")
        .bind(profile)
        .bind(library)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE media_items SET content_rating='R' WHERE id=$1")
        .bind(item)
        .execute(&pool)
        .await?;
    for path in &accessible_paths {
        let (status, _, bytes) = get(&app, Some(&tokens[0]), path).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let problem: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(problem["title"], "PROFILE_003");
    }
    sqlx::query("UPDATE media_items SET content_rating='PG' WHERE id=$1")
        .bind(item)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM profile_library_access WHERE profile_id=$1")
        .bind(profile)
        .execute(&pool)
        .await?;
    for path in &accessible_paths {
        assert_eq!(
            get(&app, Some(&tokens[0]), path).await?.0,
            StatusCode::NOT_FOUND
        );
    }
    sqlx::query("INSERT INTO profile_library_access (profile_id,library_id) VALUES ($1,$2)")
        .bind(profile)
        .bind(library)
        .execute(&pool)
        .await?;
    for path in &accessible_paths {
        assert_eq!(get(&app, Some(&tokens[0]), path).await?.0, StatusCode::OK);
    }
    sqlx::query("UPDATE play_sessions SET stopped_at=now() WHERE user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await?;
    for path in &accessible_paths {
        let (status, _, bytes) = get(&app, Some(&tokens[0]), path).await?;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let problem: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(problem["title"], "PLAY_001");
    }
    sqlx::query("DELETE FROM users WHERE id=ANY($1)")
        .bind(vec![owner, peer])
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM libraries WHERE id=$1")
        .bind(library)
        .execute(&pool)
        .await?;
    pool.close().await;
    anyhow::ensure!(
        temporary.starts_with(std::env::temp_dir())
            && temporary
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("duskcue-hls-http-")),
        "temporary fixture must stay under the intended test directory"
    );
    tokio::fs::remove_dir_all(temporary).await?;
    Ok(())
}
