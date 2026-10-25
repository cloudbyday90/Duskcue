// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#![cfg(feature = "playback-qualification")]

#[path = "support/feed_assignment.rs"]
mod assignment;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/assignment_http.rs"]
mod http;
#[path = "support/feed_lifecycle_cases.rs"]
mod lifecycle_cases;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/feed_network_cases.rs"]
mod network_cases;
#[path = "support/feed_peer.rs"]
mod peer;
#[path = "support/feed_policy_cases.rs"]
mod policy_cases;
#[path = "support/feed_storage_cases.rs"]
mod storage_cases;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 and local TLS workflow"]
async fn saved_feed_authority_and_owned_connections_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| {
        let mut case = assignment::setup(db).await?;
        policy_cases::run(&mut case, &db.url).await?;
        println!("IPTV_FEED_STAGE saved_authority_passed");
        case.admin_proof(false).await?;
        network_cases::run(&mut case).await?;
        println!("IPTV_FEED_STAGE http_tls_boundaries_passed");
        case.admin_proof(false).await?;
        lifecycle_cases::run(&mut case, &db.url).await?;
        let events:Vec<serde_json::Value>=sqlx::query_scalar("SELECT new_data FROM audit_log WHERE table_name='live_tv_assignments'").fetch_all(&case.pool).await?;
        anyhow::ensure!(events.len()>20 && events.iter().all(|v|v.as_object().is_some_and(|o|o.len()==2 && o.get("event").is_some_and(|v|v.is_string()) && o.get("household_user_id").is_some_and(|v|v.is_string()))),"destination audit included row or secret data");
        capture.check(&case.secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
        println!("IPTV_FEED_RESULT {}",serde_json::json!({"version":1,"resource_id":db.resource,"postgres_image":db.image,"encrypted_destination_authority":true,"generation_fencing":true,"hostname_tls_and_ip_identity":true,"normal_certificate_refusals":true,"bounded_http_framing":true,"no_redirect_proxy_dns_fallback":true,"owned_cancellation_and_shutdown":true,"secret_free_trace_capture":true,"safe_audit_events":events.len(),"production_fetching":false}));
        Ok(())
    }).await
}
