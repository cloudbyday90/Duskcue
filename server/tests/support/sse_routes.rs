// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{actors, database, sse_peer as peer};
use duskcue::{
    services::{event_bus::ServerEvent, runtime::HttpServer},
    state::{AppState, RateLimitConfig},
};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

pub async fn run(db: &database::Database) -> anyhow::Result<Vec<String>> {
    let state = db.state();
    let a = actors::actor(&db.pool, "standard", false).await?;
    let b = actors::actor(&db.pool, "standard", false).await?;
    let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut server = HttpServer::start(listener, app, CancellationToken::new());
    let client = peer::client()?;
    let outcome = async {
        peer::error(
            peer::request(&client, address, None, false, "", None).await?,
            401,
        )
        .await?;
        let marker = ServerEvent::new("hint", serde_json::json!({"private":"marker-private"}));
        let last = marker.id;
        state.event_bus.publish(a.user, marker);
        state.event_bus.publish(
            a.user,
            ServerEvent::new("hint", serde_json::json!({"private":"alpha-private"})),
        );
        state.event_bus.publish(
            a.user,
            ServerEvent::new("skip", serde_json::json!({"private":"filtered-private"})),
        );
        state.event_bus.publish(
            b.user,
            ServerEvent::new("hint", serde_json::json!({"private":"foreign-private"})),
        );
        let mut first = peer::Peer::new(
            peer::request(
                &client,
                address,
                Some(&a.token),
                true,
                "?types=hint",
                Some(last.to_string()),
            )
            .await?,
        )?;
        first.until("alpha-private").await?;
        let replay = String::from_utf8_lossy(&first.bytes);
        anyhow::ensure!(
            replay.contains("retry: 5000")
                && replay.contains("event: hint")
                && replay.contains("id: ")
                && !replay.contains("filtered-private")
                && !replay.contains("foreign-private"),
            "SSE replay/filter/user isolation changed"
        );
        let mut second = peer::Peer::new(
            peer::request(&client, address, Some(&b.token), false, "", None).await?,
        )?;
        second.until("retry: 5000").await?;
        let mut paused = Vec::new();
        for _ in 0..4 {
            paused.push(peer::Peer::new(
                peer::request(&client, address, Some(&a.token), false, "", None).await?,
            )?);
        }
        let refusal = peer::error(
            peer::request(&client, address, Some(&a.token), false, "", None).await?,
            429,
        )
        .await?;
        anyhow::ensure!(
            refusal["title"] == "SSE_LIMIT_REACHED",
            "SSE connection cap changed"
        );
        drop(first);
        tokio::time::timeout(Duration::from_secs(3), async {
            while state.event_bus.connection_count(a.user) != 4 {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        let mut replacement = peer::Peer::new(
            peer::request(&client, address, Some(&a.token), true, "", None).await?,
        )?;
        replacement.until("retry: 5000").await?;
        peer::error(
            peer::request(
                &client,
                address,
                Some(&b.token),
                false,
                "?types=a&types=raw-private",
                None,
            )
            .await?,
            400,
        )
        .await?;
        state.event_bus.publish(
            a.user,
            ServerEvent::new("hint", serde_json::json!({"private":"alpha-live-private"})),
        );
        state.event_bus.publish(
            b.user,
            ServerEvent::new("hint", serde_json::json!({"private":"beta-live-private"})),
        );
        second.until("beta-live-private").await?;
        state.event_streams.close();
        tokio::time::timeout(Duration::from_secs(3), state.event_streams.drain()).await??;
        anyhow::ensure!(
            state.event_bus.total_connection_count() == 0,
            "SSE drain left connection guards owned"
        );
        second.end().await?;
        replacement.end().await?;
        for paused in &mut paused {
            paused.end().await?;
        }
        anyhow::ensure!(
            !String::from_utf8_lossy(&second.bytes).contains("alpha-live-private")
                && !String::from_utf8_lossy(&replacement.bytes).contains("beta-live-private"),
            "live SSE crossed users"
        );
        let late = peer::request(&client, address, Some(&b.token), false, "", None).await?;
        anyhow::ensure!(
            late.headers().get("retry-after").is_some_and(|v| v == "5"),
            "SSE shutdown backoff changed"
        );
        anyhow::ensure!(
            peer::error(late, 503).await?["title"] == "SSE_UNAVAILABLE",
            "SSE shutdown error leaked internals"
        );
        peer::error(
            peer::request(&client, address, None, false, "", None).await?,
            401,
        )
        .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    state.event_streams.close();
    server.close();
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::try_join!(server.wait(), async {
            state
                .event_streams
                .drain()
                .await
                .map_err(|_| duskcue::services::runtime::ShutdownError)
        })
    })
    .await??;
    outcome?;
    outer_rate_limit(state.clone()).await?;
    println!("SSE_LIFECYCLE_STAGE authenticated_tcp_and_privacy_passed");
    Ok(vec![
        a.token,
        b.token,
        "alpha-private".into(),
        "filtered-private".into(),
        "foreign-private".into(),
        "alpha-live-private".into(),
        "beta-live-private".into(),
        "raw-private".into(),
        db.password.clone(),
    ])
}

async fn outer_rate_limit(mut state: AppState) -> anyhow::Result<()> {
    state.rate_limits =
        std::sync::Arc::new(duskcue::middleware::RateLimitState::new(&RateLimitConfig {
            global_per_minute: 1,
            global_burst: 1,
            ..Default::default()
        }));
    let app = duskcue::router::build_router(state.clone()).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    let mut server = HttpServer::start(listener, app, CancellationToken::new());
    let client = peer::client()?;
    let result = async {
        peer::error(
            peer::request(&client, address, None, false, "", None).await?,
            401,
        )
        .await?;
        peer::error(
            peer::request(&client, address, None, false, "", None).await?,
            429,
        )
        .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await;
    server.close();
    tokio::time::timeout(Duration::from_secs(3), server.wait()).await??;
    result
}
