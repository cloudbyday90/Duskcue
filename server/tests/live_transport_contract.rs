// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/live_transport_auth.rs"]
mod auth;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/live_transport_media.rs"]
mod media;
#[path = "support/live_transport_upstream.rs"]
mod upstream;

use axum::http::StatusCode;
use duskcue::{
    config::BootstrapConfig,
    domains::live_tv::transport::{Source, TransportError, TransportOwner},
    services::encryption::EncryptionKey,
    state::AppState,
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn source(upstream: &upstream::Upstream, mode: &str) -> Source {
    Source::new(
        &format!(
            "http://{}/{mode}?credential=private-fixture-secret",
            upstream.address
        ),
        upstream.address,
    )
    .unwrap()
}

async fn idle(
    owner: &TransportOwner,
    root: &Path,
    upstream: &upstream::Upstream,
) -> anyhow::Result<()> {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if owner.idle() && upstream.active() == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await?;
    anyhow::ensure!(
        media::workers(root).await?.is_empty(),
        "live worker still present"
    );
    anyhow::ensure!(
        tokio::fs::read_dir(root)
            .await?
            .next_entry()
            .await?
            .is_none(),
        "live cache retained"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires isolated managed Linux runtime and disposable PostgreSQL"]
async fn supervised_mpegts_transport_contract() -> anyhow::Result<()> {
    anyhow::ensure!(cfg!(target_os = "linux"), "managed Linux runtime required");
    anyhow::ensure!(
        std::env::var("DUSKCUE_LIVE_TRANSPORT_TESTS").as_deref() == Ok("disposable"),
        "explicit disposable opt-in required"
    );
    let resource = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
    anyhow::ensure!(
        std::env::var("HTTP_PROXY").as_deref() == Ok("http://127.0.0.1:1"),
        "proxy refusal fixture missing"
    );
    let logs = logs::capture()?;
    let root = std::path::PathBuf::from("/fixtures/live");
    tokio::fs::create_dir(&root).await?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(
        database.starts_with("duskcue_live_transport_test_"),
        "isolated transport database required"
    );
    sqlx::migrate!("./migrations").run(&pool).await?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS audit_log_transport_test PARTITION OF audit_log DEFAULT",
    )
    .execute(&pool)
    .await?;
    let actor = auth::actor(&pool, "owner", "standard").await?;
    println!("TRANSPORT_STAGE database_ready");
    let other = auth::actor(&pool, "owner", "standard").await?;
    let member = auth::actor(&pool, "member", "standard").await?;
    let kids = auth::actor(&pool, "owner", "kids").await?;
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: "/data".into(),
        cache_dir: "/cache".into(),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let (key, _) = EncryptionKey::generate();
    let state = AppState::new(pool.clone(), bootstrap, recorder.handle(), key);
    let upstream = upstream::Upstream::start(media::source().await?).await?;
    println!("TRANSPORT_STAGE source_ready");
    let owner = TransportOwner::new(&root).await?;
    let outcome: anyhow::Result<_> = async {
        let mut session = owner.start(source(&upstream, "live"))?;
        let pid = session.ready().await?;
        println!("TRANSPORT_STAGE rolling_started");
        let identity = tokio::fs::read(format!("/proc/{pid}/stat")).await?;
        anyhow::ensure!(
            !identity.is_empty() && media::workers(&root).await?.contains(&pid),
            "worker identity missing"
        );
        let output = session.output();
        let app = auth::router(state.clone(), &actor, output.clone());
        for (token, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (Some(member.token.as_str()), StatusCode::FORBIDDEN),
            (Some(kids.token.as_str()), StatusCode::FORBIDDEN),
            (Some(other.token.as_str()), StatusCode::FORBIDDEN),
        ] {
            anyhow::ensure!(
                auth::get(&app, token.map(|token| (token, false)), "manifest.m3u8")
                    .await?
                    .0
                    == expected,
                "authorization mismatch"
            );
        }
        let first = output.snapshot().await?;
        let expired_name = first.segments[0].clone();
        let deadline = Instant::now() + Duration::from_secs(110);
        let mut peak_bytes = 0;
        let mut peak_files = 0;
        let rotated = loop {
            let snapshot = output.snapshot().await?;
            peak_bytes = peak_bytes.max(snapshot.bytes);
            peak_files = peak_files.max(snapshot.files);
            anyhow::ensure!(
                snapshot.bytes <= 32 * 1024 * 1024 && snapshot.files <= 24,
                "rolling output budget exceeded"
            );
            if snapshot.sequence >= 30 {
                break snapshot;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "two complete rolling windows were not observed"
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        anyhow::ensure!(
            auth::get(&app, Some((&actor.token, true)), &expired_name)
                .await?
                .0
                == StatusCode::NOT_FOUND,
            "retired segment retained"
        );
        let mut decoded = Vec::new();
        println!("TRANSPORT_STAGE rotations_complete");
        for cookie in [true, false] {
            let current = output.snapshot().await?;
            let (status, manifest) =
                auth::get(&app, Some((&actor.token, cookie)), "manifest.m3u8").await?;
            anyhow::ensure!(
                status == StatusCode::OK
                    && !String::from_utf8_lossy(&manifest).contains("private-fixture-secret"),
                "manifest response invalid"
            );
            let (status, mut bytes) =
                auth::get(&app, Some((&actor.token, cookie)), "init.mp4").await?;
            anyhow::ensure!(status == StatusCode::OK, "initialization unavailable");
            let (status, fragment) = auth::get(
                &app,
                Some((&actor.token, cookie)),
                current.segments.last().unwrap(),
            )
            .await?;
            anyhow::ensure!(status == StatusCode::OK, "fragment unavailable");
            bytes.extend(fragment);
            let file = root.parent().unwrap().join(format!("decoded-{cookie}.mp4"));
            tokio::fs::write(&file, bytes).await?;
            decoded.push(media::decode(&file).await?);
            tokio::fs::remove_file(file).await?;
        }
        let alternate = uuid::Uuid::now_v7();
        sqlx::query("INSERT INTO user_profiles (id,owner_user_id,name,profile_type,is_default) VALUES ($1,$2,'Alternate','standard',false)")
            .bind(alternate).bind(actor.user).execute(&pool).await?;
        sqlx::query("UPDATE user_sessions SET active_profile_id=$1 WHERE user_id=$2").bind(alternate).bind(actor.user).execute(&pool).await?;
        anyhow::ensure!(auth::get(&app, Some((&actor.token, false)), "manifest.m3u8").await?.0 == StatusCode::FORBIDDEN, "another profile gained output access");
        sqlx::query("UPDATE user_sessions SET active_profile_id=$1 WHERE user_id=$2").bind(actor.profile).bind(actor.user).execute(&pool).await?;
        sqlx::query(
            "UPDATE user_sessions SET expires_at=now()-interval '1 second' WHERE user_id=$1",
        )
        .bind(actor.user)
        .execute(&pool)
        .await?;
        anyhow::ensure!(
            auth::get(&app, Some((&actor.token, false)), "manifest.m3u8")
                .await?
                .0
                == StatusCode::UNAUTHORIZED,
            "expired credential accepted"
        );
        let stopped = Instant::now();
        session.stop().await?;
        idle(&owner, &root, &upstream).await?;
        anyhow::ensure!(
            !Path::new(&format!("/proc/{pid}")).exists()
                && output.read("manifest.m3u8").await.is_err(),
            "stopped resources remain"
        );
        let stop_ms = stopped.elapsed().as_millis();
        for (mode, expected) in [
            ("redirect", TransportError::Response),
            ("html", TransportError::Response),
            ("invalid", TransportError::Input),
            ("eof", TransportError::Ended),
            ("stall", TransportError::Connection),
        ] {
            let mut failed = owner.start(source(&upstream, mode))?;
            let error = tokio::time::timeout(Duration::from_secs(12), failed.finished())
                .await?
                .unwrap_err();
            anyhow::ensure!(error == expected, "wrong fixed error for {mode}: {error}");
            idle(&owner, &root, &upstream).await?;
        }
        let mut cancelled = owner.start(source(&upstream, "stall"))?;
        cancelled.stop().await?;
        idle(&owner, &root, &upstream).await?;
        let mut abandoned = owner.start(source(&upstream, "live"))?;
        abandoned.ready().await?;
        drop(abandoned);
        idle(&owner, &root, &upstream).await?;
        let mut blocked = owner.start(source(&upstream, "live"))?;
        let blocked_pid = blocked.ready().await?;
        anyhow::ensure!(
            media::workers(&root).await?.contains(&blocked_pid),
            "blocked worker identity missing"
        );
        media::pause(blocked_pid)?;
        tokio::time::sleep(Duration::from_secs(3)).await;
        anyhow::ensure!(
            blocked.output().input_pending(),
            "input pipe did not become backpressured"
        );
        blocked.stop().await?;
        idle(&owner, &root, &upstream).await?;
        let mut budget = owner.start(source(&upstream, "live"))?;
        budget.ready().await?;
        let directory = tokio::fs::read_dir(&root)
            .await?
            .next_entry()
            .await?
            .unwrap()
            .path();
        for n in 90..99 {
            match std::fs::File::create(directory.join(format!("seg_{n:08}.m4s"))) {
                Ok(file) => file.set_len(4 * 1024 * 1024)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::ensure!(
            tokio::time::timeout(Duration::from_secs(10), budget.finished()).await?
                == Err(TransportError::Cache),
            "cache byte ceiling did not stop transport"
        );
        idle(&owner, &root, &upstream).await?;
        let mut file_budget = owner.start(source(&upstream, "live"))?;
        file_budget.ready().await?;
        let directory = tokio::fs::read_dir(&root)
            .await?
            .next_entry()
            .await?
            .unwrap()
            .path();
        for n in 100..125 {
            match std::fs::File::create(directory.join(format!("seg_{n:08}.m4s"))) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => break,
                Err(e) => return Err(e.into()),
            }
        }
        anyhow::ensure!(
            tokio::time::timeout(Duration::from_secs(10), file_budget.finished()).await?
                == Err(TransportError::Cache),
            "cache file ceiling did not stop transport"
        );
        idle(&owner, &root, &upstream).await?;
        let dropped_owner = TransportOwner::new(&root).await?;
        let mut dropped_session = dropped_owner.start(source(&upstream, "live"))?;
        dropped_session.ready().await?;
        drop(dropped_owner);
        tokio::time::timeout(Duration::from_secs(10), dropped_session.finished()).await??;
        idle(&owner, &root, &upstream).await?;
        let _shutdown_session = owner.start(source(&upstream, "stall"))?;
        owner.shutdown().await?;
        idle(&owner, &root, &upstream).await?;
        anyhow::ensure!(
            owner.start(source(&upstream, "live")).is_err() && upstream.traps() == 0,
            "closed admissions or redirect boundary failed"
        );
        let play_sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM play_sessions")
            .fetch_one(&pool)
            .await?;
        anyhow::ensure!(
            play_sessions == 0,
            "transport created catalog playback sessions"
        );
        Ok(json!({
            "version": 1, "resource_id": resource, "rotated_sequence": rotated.sequence,
            "peak_cache_bytes": peak_bytes, "peak_cache_files": peak_files,
            "decoded": decoded, "stop_ms": stop_ms, "cookie_and_bearer": true,
            "scope_refusals": true, "expired_refusal": true, "redirect_requests": 0,
            "worker_cleanup": true, "upstream_cleanup": true, "cache_cleanup": true,
            "blocked_pipe_cancel": true, "budget_refusal": true, "shutdown_joined": true,
            "catalog_sessions": play_sessions, "ambient_proxy_ignored": true,
            "owner_drop_joined": true, "file_budget_refusal": true, "eof_refusal": true
        }))
    }
    .await;
    let shutdown = owner.shutdown().await;
    let upstream_stopped = upstream.stop().await;
    pool.close().await;
    shutdown?;
    upstream_stopped?;
    logs.check(&[&actor.token, &other.token, &member.token, &kids.token])?;
    println!("LIVE_TRANSPORT_RESULT {}", outcome?);
    Ok(())
}
