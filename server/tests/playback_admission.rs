// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/feed_assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "support/playback_admission_cases.rs"]
mod cases;
#[path = "support/catalog_fixture.rs"]
#[allow(dead_code)]
mod catalog_fixture;
#[path = "support/admission_commit_proxy.rs"]
mod commit_proxy;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/playback_admission_faults.rs"]
mod faults;
#[path = "support/catalog_feeds.rs"]
#[allow(dead_code)]
mod feeds;
#[path = "support/playback_admission_fixture.rs"]
mod fixture;
#[path = "support/assignment_http.rs"]
#[allow(dead_code, unused_imports)]
mod http;
#[path = "support/playback_admission_live.rs"]
mod live;
#[path = "support/feed_peer.rs"]
#[allow(dead_code)]
mod peer;
#[path = "support/playback_admission_resources.rs"]
mod resources;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 workflow"]
async fn shared_viewer_ledger_and_owned_resources() -> anyhow::Result<()> {
    database::run(async |db| {
        anyhow::ensure!(!db.resource.is_empty() && db.image.starts_with("sha256:"),"owned database identity missing");
        cases::run(db).await?;
        live::run(db).await?;
        faults::run(db).await?;
        resources::run(db).await?;
        println!("PLAYBACK_ADMISSION_RESULT durable_claims=true owned_cleanup=true production_enforcement=false");
        Ok(())
    }).await
}

#[test]
#[ignore = "owned child subprocess only"]
fn owned_resource_child() {
    assert_eq!(
        std::env::var("DUSKCUE_ADMISSION_CHILD").as_deref(),
        Ok("owned")
    );
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
