// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/assignment_admin_cases.rs"]
mod admin_cases;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/assignment_http.rs"]
mod http;
#[path = "support/assignment_lifecycle_cases.rs"]
mod lifecycle_cases;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/assignment_owner_cases.rs"]
mod owner_cases;
#[path = "support/assignment_revocation_cases.rs"]
mod revocation_cases;

use serde_json::json;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 workflow"]
async fn live_tv_assignment_production_routes_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| contract(db, &capture).await).await
}

async fn contract(db: &database::Database, capture: &logs::Capture) -> anyhow::Result<()> {
    let state = db.state();
    let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
    let admin = http::actor(&db.pool, "standard", true).await?;
    sqlx::query("UPDATE users SET role='admin' WHERE id=$1")
        .bind(admin.user)
        .execute(&db.pool)
        .await?;
    let mut owner = http::actor(&db.pool, "standard", true).await?;
    owner.cookie = true;
    let other = http::actor(&db.pool, "standard", false).await?;
    let standard = Uuid::now_v7();
    let kids = Uuid::now_v7();
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type) VALUES ($1,$2,'Viewer','standard'),($3,$2,'Child','kids')").bind(standard).bind(owner.user).bind(kids).execute(&db.pool).await?;
    let secrets = vec![
        db.password.clone(),
        http::PASSWORD.to_string(),
        admin.token.clone(),
        owner.token.clone(),
        other.token.clone(),
        http::PLAYLIST.into(),
        http::GUIDE.into(),
        http::ROTATED_PLAYLIST.into(),
        http::ROTATED_GUIDE.into(),
    ];
    let mut case = http::Case {
        app,
        state,
        pool: db.pool.clone(),
        admin,
        owner,
        other,
        standard,
        kids,
        secrets,
    };
    admin_cases::run(&mut case).await?;
    println!("IPTV_ASSIGNMENT_STAGE administrator_contract_passed");
    owner_cases::run(&mut case).await?;
    revocation_cases::run(&mut case, &db.url).await?;
    println!("IPTV_ASSIGNMENT_STAGE owner_revocation_contract_passed");
    lifecycle_cases::run(&mut case).await?;
    let events: Vec<serde_json::Value> =
        sqlx::query_scalar("SELECT new_data FROM audit_log WHERE table_name='live_tv_assignments'")
            .fetch_all(&db.pool)
            .await?;
    anyhow::ensure!(
        events.len() >= 10
            && events
                .iter()
                .all(|e| e.as_object().is_some_and(|v| v.len() == 2
                    && v.get("event").is_some_and(|v| v.is_string())
                    && v.get("household_user_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|v| v == case.owner.user.to_string()
                            || v == case.admin.user.to_string()))),
        "assignment audit copied row/secret data"
    );
    while case
        .state
        .rate_limits
        .ip_global
        .check_key(&std::net::IpAddr::from([0, 0, 0, 1]))
        .is_ok()
    {}
    http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &case.admin_path(),
        None,
        &[],
    )
    .await?
    .expect(axum::http::StatusCode::TOO_MANY_REQUESTS)?;
    capture.check(&case.secrets.iter().map(String::as_str).collect::<Vec<_>>())?;
    println!(
        "IPTV_ASSIGNMENT_RESULT {}",
        json!({"version":1,"resource_id":db.resource,"postgres_image":db.image,"encrypted_storage":true,"real_password_proof":true,"cookie_and_bearer":true,"administrator_proof_and_permission":true,"owner_delegations":true,"conditional_updates":true,"rotation_and_replacement":true,"kids_refused":true,"observed_capability_and_delegation_races":true,"transaction_rollback":true,"no_upstream_connection":true,"safe_audit_events":events.len(),"outer_rate_limit_no_store":true})
    );
    Ok(())
}
