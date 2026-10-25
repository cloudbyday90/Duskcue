// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::database;
use duskcue::services::runtime::{self, HttpServer};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub async fn run(db: &database::Database) -> anyhow::Result<()> {
    let state = db.state();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let (start, end) = (entered.clone(), release.clone());
    let app = axum::Router::new().route(
        "/hold",
        axum::routing::get(move || {
            let (start, end) = (start.clone(), end.clone());
            async move {
                start.notify_one();
                end.notified().await;
                "joined"
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut http = HttpServer::start(listener, app, CancellationToken::new());
    let client = tokio::spawn(async move {
        let mut socket = TcpStream::connect(address).await?;
        socket
            .write_all(b"GET /hold HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut bytes = Vec::new();
        socket.take(4096).read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    });
    let admitted = tokio::time::timeout(Duration::from_secs(3), entered.notified()).await;
    let started = Instant::now();
    let tracker = TaskTracker::new();
    let outcome = tokio::time::timeout(
        Duration::from_secs(35),
        runtime::shutdown(&state, &tracker, &mut http),
    )
    .await;
    let correct = admitted.is_ok()
        && matches!(outcome, Ok(Err(_)))
        && started.elapsed() >= Duration::from_secs(29)
        && !db.pool.is_closed()
        && !http.finished();
    release.notify_one();
    let response = tokio::time::timeout(Duration::from_secs(3), client).await???;
    tokio::time::timeout(
        Duration::from_secs(5),
        runtime::shutdown(&state, &tracker, &mut http),
    )
    .await??;
    anyhow::ensure!(
        correct && response.ends_with(b"joined"),
        "HTTP deadline falsely certified cleanup or closed dependencies before the join"
    );
    println!(
        "IPTV_REFRESH_HTTP_RESULT {}",
        serde_json::json!({"version":1,"resource_id":db.resource,"real_http_deadline_failure":true,"pool_open_until_join":true,"fixture_cleanup_joined":true})
    );
    Ok(())
}
