// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/owner_authenticator.rs"]
mod authenticator;
#[path = "support/owner_capacity.rs"]
mod capacity;
#[path = "support/owner_context_cases.rs"]
mod context_cases;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/owner_http.rs"]
mod http;
#[path = "support/owner_legacy_cases.rs"]
mod legacy_cases;
#[path = "support/live_transport_logs.rs"]
mod logs;
#[path = "support/owner_operator.rs"]
mod operator_cases;
#[path = "support/owner_passkey_cases.rs"]
mod passkey_cases;
#[path = "support/owner_password_cases.rs"]
mod password_cases;

use serde_json::json;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 workflow"]
async fn household_owner_production_routes_contract() -> anyhow::Result<()> {
    let capture = logs::capture()?;
    database::run(async |db| contract(db, &capture).await).await
}

async fn contract(db: &database::Database, capture: &logs::Capture) -> anyhow::Result<()> {
    let pool = db.pool.clone();
    let state = db.state();
    let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
    let mut secrets = vec![db.password.clone(), http::PASSWORD.to_string()];
    let cli_actor = http::actor(&pool, "standard", false).await?;
    secrets.push(cli_actor.token.clone());
    operator_cases::run(&db.url, &db.root, &pool, cli_actor.user, &mut secrets).await?;
    password_cases::run(&app, &state, &pool, &mut secrets).await?;
    context_cases::run(&app, &pool, &mut secrets).await?;
    println!("OWNER_SECURITY_STAGE password_contract_passed");
    legacy_cases::run(&app, &pool, &mut secrets).await?;
    passkey_cases::run(&app, &state, &pool, &mut secrets).await?;
    println!("OWNER_SECURITY_STAGE passkey_contract_passed");
    let events: Vec<serde_json::Value> = sqlx::query_scalar(
        "SELECT new_data FROM audit_log WHERE table_name='household_owner_state'",
    )
    .fetch_all(&pool)
    .await?;
    anyhow::ensure!(
        events.len() > 20
            && events.iter().all(|e| e
                .as_object()
                .is_some_and(|o| o.len() == 1 && o.contains_key("event"))),
        "safe security audit events missing"
    );
    while state
        .rate_limits
        .ip_global
        .check_key(&std::net::IpAddr::from([0, 0, 0, 1]))
        .is_ok()
    {}
    anyhow::ensure!(
        http::call(
            &app,
            Some(&cli_actor),
            "GET",
            &format!("{}/status", http::ROOT),
            None
        )
        .await?
        .0 == axum::http::StatusCode::TOO_MANY_REQUESTS,
        "outer rate-limit response was not exercised"
    );
    let refs = secrets.iter().map(String::as_str).collect::<Vec<_>>();
    capture.check(&refs)?;
    println!(
        "OWNER_SECURITY_RESULT {}",
        json!({"version":1,"resource_id":db.resource,"postgres_image":db.image,"password":true,"signed_webauthn":true,"cookie_and_bearer_rotation":true,"operator_command":true,"outer_rate_limit_no_store":true,"stable_session_bindings":true,"legacy_preferences_and_kids_pin":true,"factor_capacity_and_recovery":true,"protected_mutations":true,"recovery":true,"replay_refusal":true,"revocation_race":true,"safe_audit_events":events.len()})
    );
    Ok(())
}
