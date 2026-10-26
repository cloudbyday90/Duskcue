// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::{
    config::BootstrapConfig,
    services::{
        encryption::EncryptionKey,
        runtime::{self, HttpServer},
    },
    state::AppState,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires guarded disposable PostgreSQL and explicit VOD qualification"]
async fn root_deadline_keeps_database_open_until_owned_vod_file_completion() -> anyhow::Result<()> {
    anyhow::ensure!(std::env::var("DUSKCUE_TEST_RESOURCE_ID").is_ok());
    anyhow::ensure!(std::env::var("DUSKCUE_TRANSCODE_ACCESS_TESTS").as_deref() == Ok("disposable"));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .connect(&std::env::var("DUSKCUE_DATABASE_URL")?)
        .await?;
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await?;
    anyhow::ensure!(database.starts_with("duskcue_transcode_test"));
    let parent = std::env::temp_dir().canonicalize()?;
    let root = parent.join(format!("duskcue-vod-root-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&root).await?;
    let path = root.join("held.m4s");
    tokio::fs::write(&path, b"owned file").await?;
    let bootstrap = BootstrapConfig {
        database_url: None,
        bind_address: "127.0.0.1".into(),
        port: 0,
        data_dir: root.join("data"),
        cache_dir: root.join("cache"),
        log_level: "error".into(),
        environment: "test".into(),
        encryption_key: None,
        geoip_license_key: None,
    };
    let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
    let state = AppState::new(
        pool.clone(),
        bootstrap,
        recorder.handle(),
        EncryptionKey::generate().0,
    );
    let app = crate::domains::playback::router(state.clone()).with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let mut http = HttpServer::start(listener, app, CancellationToken::new());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let owner = state.transcode_manager.output.clone();
    let (start, finish) = (entered.clone(), release.clone());
    let response = tokio::spawn(async move {
        owner
            .response(None, move |ctx| {
                Box::pin(async move {
                    ctx.open(&path, None).await?;
                    start.notify_one();
                    finish.notified().await;
                    ctx.check()?;
                    Ok(Prepared {
                        status: axum::http::StatusCode::OK,
                        headers: Default::default(),
                        content: Content::Empty,
                    })
                })
            })
            .await
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let tracker = TaskTracker::new();
    let started = std::time::Instant::now();
    let result = runtime::shutdown(&state, &tracker, &mut http).await;
    let refused =
        result.is_err() && started.elapsed() >= Duration::from_secs(29) && !pool.is_closed();
    release.notify_one();
    anyhow::ensure!(
        tokio::time::timeout(Duration::from_secs(3), response)
            .await??
            .is_err()
    );
    tokio::time::timeout(
        Duration::from_secs(5),
        runtime::shutdown(&state, &tracker, &mut http),
    )
    .await??;
    anyhow::ensure!(refused && pool.is_closed());
    let resolved = root.canonicalize()?;
    anyhow::ensure!(resolved.parent() == Some(parent.as_path()) && resolved == root);
    tokio::fs::remove_dir_all(resolved).await?;
    println!(
        "VOD_ROOT_RESULT {}",
        serde_json::json!({"real_postgresql":true,"real_file_held":true,"thirty_second_failure":true,"database_open_until_completion":true,"retry_joined":true})
    );
    Ok(())
}
