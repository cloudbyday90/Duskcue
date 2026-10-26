// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/feed_assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "support/catalog_http.rs"]
mod catalog_http;
#[path = "support/household_database.rs"]
#[allow(dead_code)]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/catalog_feeds.rs"]
mod feeds;
#[path = "support/catalog_fixture.rs"]
mod fixture;
#[path = "support/assignment_http.rs"]
#[allow(dead_code, unused_imports)]
mod http;
#[path = "support/live_transport_logs.rs"]
#[allow(dead_code)]
mod logs;
#[path = "support/feed_peer.rs"]
#[allow(dead_code)]
mod peer;
#[path = "support/catalog_routes.rs"]
mod routes;

#[path = "support/catalog_context_cases.rs"]
mod context_cases;
#[path = "support/catalog_integrity_cases.rs"]
mod integrity_cases;
#[path = "support/catalog_isolation_cases.rs"]
mod isolation_cases;
#[path = "support/catalog_lifecycle_cases.rs"]
mod lifecycle_cases;
#[path = "support/catalog_race_cases.rs"]
mod race_cases;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted owned PostgreSQL18 and real HTTP/crypto/catalog workflow"]
async fn profile_scoped_catalog_contract() -> anyhow::Result<()> {
    let capture = logs::capture_with_filter(
        "reqwest=trace,hyper_util=trace,hyper=trace,rustls=trace,duskcue=trace,tower_http=trace",
    )?;
    database::run(async |db| {
        let mut fixture=fixture::setup(db).await?;
        let outcome=async {
            let requests=fixture.source.count();
            let inventory=routes::run(&fixture).await?;
            fixture.case.secrets.push(inventory.cursor.clone());
            context_cases::run(&mut fixture,&inventory).await?;
            integrity_cases::run(&fixture,&inventory).await?;
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address=listener.local_addr()?;
            let mut server=duskcue::services::runtime::HttpServer::start(listener,fixture.case.app.clone(),tokio_util::sync::CancellationToken::new());
            let tcp=async {catalog_http::tcp(address,&fixture.case.owner).await?;catalog_http::tcp(address,&fixture.viewer).await}.await;
            server.close();tokio::time::timeout(std::time::Duration::from_secs(5),server.wait()).await??;
            tcp?;
            anyhow::ensure!(fixture.source.count()==requests,"catalog reads performed upstream requests");
            isolation_cases::run(&mut fixture,&inventory).await?;
            lifecycle_cases::run(&mut fixture,&inventory).await?;
            race_cases::run(&mut fixture,&db.url).await?;
            capture.check(&fixture.case.secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
            println!("IPTV_CATALOG_RESULT {}",serde_json::json!({"version":1,"resource_id":db.resource,"selected_profile_reads":true,"safe_metadata":true,"no_upstream_reads":true}));
            Ok::<_,anyhow::Error>(())
        }.await;
        fixture.case.state.live_tv_catalog.close();
        fixture.source.finish().await?;
        outcome
    }).await
}
