// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/feed_assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "support/import_commit_proxy.rs"]
mod commit_proxy;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/assignment_http.rs"]
#[allow(dead_code, unused_imports)]
mod http;
#[path = "support/import_fixture.rs"]
mod import_fixture;
#[path = "support/import_lifecycle_cases.rs"]
mod lifecycle_cases;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/import_network_cases.rs"]
mod network_cases;
#[path = "support/feed_peer.rs"]
#[allow(dead_code)]
mod peer;
#[path = "support/import_storage_cases.rs"]
mod storage_cases;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 and local TLS workflow"]
async fn bounded_imports_and_atomic_snapshots_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| {
        let mut case=assignment::setup(db).await?;
        case.secrets.extend(["stream-private","Channel-private","Guide-private","Programme-private","Partial-private","shared-guide","raw-private","rotated-private"].map(str::to_owned));
        storage_cases::run(&mut case).await?;
        case.admin_proof(false).await?;
        network_cases::run(&mut case).await?;
        case.admin_proof(false).await?;
        lifecycle_cases::run(&mut case,&db.url).await?;
        let events:Vec<serde_json::Value>=sqlx::query_scalar("SELECT new_data FROM audit_log WHERE table_name='live_tv_import_state' AND user_id IS NULL").fetch_all(&case.pool).await?;
        anyhow::ensure!(!events.is_empty() && events.iter().all(|v|v.as_object().is_some_and(|o|o.len()==6 && o.get("event").is_some_and(|v|v=="published"))),"import audit attribution or redaction incorrect");
        capture.check(&case.secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
        println!("IPTV_IMPORT_RESULT {}",serde_json::json!({"version":1,"resource_id":db.resource,"postgres_image":db.image,"bounded_encrypted_atomic_imports":true,"safe_audit_events":events.len(),"production_refresh":false,"client_reads":false,"playback":false}));
        Ok(())
    }).await
}
