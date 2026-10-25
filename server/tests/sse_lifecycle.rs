// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/household_actor.rs"]
#[allow(dead_code)]
mod actors;
#[path = "support/household_database.rs"]
#[allow(dead_code)]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/sse_routes.rs"]
mod routes;
#[path = "support/sse_peer.rs"]
mod sse_peer;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted PostgreSQL18 and owned SSE TCP workflow"]
async fn authenticated_sse_lifecycle_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| {
        let secrets=routes::run(db).await?;
        capture.check(&secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
        println!("SSE_LIFECYCLE_RESULT {}",serde_json::json!({"version":1,"resource_id":db.resource,"authenticated_tcp":true,"no_store":true,"retained_sse_cleanup":true}));
        Ok(())
    }).await
}
