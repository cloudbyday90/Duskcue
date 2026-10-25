// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::Case;
use duskcue::services::runtime::{self, HttpServer};
use sqlx::postgres::PgPoolOptions;
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::Notify,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

pub async fn run(case: &Case, url: &str) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new().max_connections(2).connect(url).await?;
    let held = pool.acquire().await?;
    let mut state = case.state.clone();
    state.pool = pool.clone();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let start = entered.clone();
    let finish = release.clone();
    let app = axum::Router::new().route(
        "/hold",
        axum::routing::get(move || {
            let start = start.clone();
            let finish = finish.clone();
            async move {
                start.notify_one();
                finish.notified().await;
                "joined"
            }
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut http = HttpServer::start(listener, app, CancellationToken::new());
    let client = tokio::spawn(async move {
        let mut stream = TcpStream::connect(address).await?;
        stream
            .write_all(b"GET /hold HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await?;
        let mut data = Vec::new();
        stream.take(4096).read_to_end(&mut data).await?;
        Ok::<_, std::io::Error>(data)
    });
    tokio::time::timeout(Duration::from_secs(3), entered.notified()).await?;
    let shutdown =
        tokio::spawn(
            async move { runtime::shutdown(&state, &TaskTracker::new(), &mut http).await },
        );
    tokio::time::sleep(Duration::from_millis(100)).await;
    anyhow::ensure!(
        !pool.is_closed() && !shutdown.is_finished(),
        "pool was closed before actual HTTP completion"
    );
    release.notify_one();
    anyhow::ensure!(
        tokio::time::timeout(Duration::from_secs(3), client)
            .await???
            .ends_with(b"joined"),
        "in-flight HTTP request did not drain"
    );
    tokio::time::timeout(Duration::from_secs(3), async {
        while !pool.is_closed() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    anyhow::ensure!(
        !shutdown.is_finished(),
        "shutdown reported pool completion while a connection was owned"
    );
    drop(held);
    tokio::time::timeout(Duration::from_secs(3), shutdown).await???;
    println!("IPTV_REFRESH_STAGE actual_http_and_pool_join_order_passed");
    Ok(())
}
