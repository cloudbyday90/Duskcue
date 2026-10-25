// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/refresh_admission_cases.rs"]
mod admission_cases;
#[path = "support/feed_assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "support/refresh_boundary_cases.rs"]
mod boundary_cases;
#[path = "support/refresh_commit_cases.rs"]
mod commit_cases;
#[path = "support/import_commit_proxy.rs"]
mod commit_proxy;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/assignment_http.rs"]
#[allow(dead_code, unused_imports)]
mod http;
#[path = "support/refresh_http_deadline.rs"]
mod http_deadline;
#[path = "support/import_fixture.rs"]
#[allow(dead_code)]
mod import_fixture;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/feed_peer.rs"]
#[allow(dead_code)]
mod peer;
#[path = "support/refresh_signal.rs"]
mod process_signal;
#[path = "support/refresh_publication_case.rs"]
mod publication_case;
#[path = "support/refresh_cases.rs"]
mod refresh_cases;
#[path = "support/refresh_root.rs"]
mod root_cases;
#[path = "support/refresh_runtime_case.rs"]
mod runtime_case;
#[path = "support/refresh_stop_request.rs"]
mod stop_request;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted PostgreSQL18 and owned HTTP/TLS/process workflow"]
async fn protected_manual_refresh_and_root_lifecycle_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| {
        let (key,encoded)=duskcue::services::encryption::EncryptionKey::generate();
        let mut case=assignment::setup_state(db,db.state_with_key(key)).await?;
        case.secrets.push(encoded.clone());
        case.secrets.extend(["stream-private","Channel-private","Guide-private","Programme-private","Partial-private","shared-guide","raw-private","rotated-private"].map(str::to_owned));
        refresh_cases::run(&mut case,&db.url).await?;
        commit_cases::run(&mut case,&db.url).await?;
        boundary_cases::run(&mut case).await?;
        root_cases::run(db,&mut case,&encoded).await?;
        runtime_case::run(&case,&db.url).await?;
        capture.check(&case.secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
        println!("IPTV_REFRESH_RESULT {}",serde_json::json!({"version":1,"resource_id":db.resource,"postgres_image":db.image,"protected_durable_admission":true,"root_owned":true,"playback":false}));Ok(())
    }).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted PostgreSQL18 and the actual sixty-second pool deadline"]
async fn pool_close_deadline_reports_incomplete_shutdown() -> anyhow::Result<()> {
    use duskcue::services::runtime::{self, HttpServer};
    use std::time::{Duration, Instant};
    use tokio_util::{sync::CancellationToken, task::TaskTracker};

    database::run(async |db| {
        let state = db.state();
        duskcue::domains::live_tv::refresh::RefreshManager::attach(&state)?;
        let held = db.pool.acquire().await?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let mut http = HttpServer::start(listener, axum::Router::new(), CancellationToken::new());
        let started = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(65),
            runtime::shutdown(&state, &TaskTracker::new(), &mut http),
        ).await?;
        anyhow::ensure!(outcome.is_err() && started.elapsed() >= Duration::from_secs(59)
            && http.finished() && db.pool.is_closed(), "pool deadline falsely reported complete shutdown");
        drop(held);
        tokio::time::timeout(Duration::from_secs(3), db.pool.close()).await?;
        println!("IPTV_REFRESH_POOL_RESULT {}", serde_json::json!({"version":1,"resource_id":db.resource,"pool_deadline_failure":true,"actual_http_joined":true,"released_connection_joined":true}));
        Ok(())
    }).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted PostgreSQL18 and the actual thirty-second HTTP deadline"]
async fn http_drain_deadline_keeps_dependencies_open() -> anyhow::Result<()> {
    database::run(async |db| http_deadline::run(db).await).await
}
