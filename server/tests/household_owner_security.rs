// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/owner_authenticator.rs"]
mod authenticator;
#[path = "support/owner_capacity.rs"]
mod capacity;
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

use duskcue::{config::BootstrapConfig, services::encryption::EncryptionKey, state::AppState};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::time::{Duration, Instant};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 workflow"]
async fn household_owner_production_routes_contract() -> anyhow::Result<()> {
    let resource = std::env::var("DUSKCUE_TEST_RESOURCE_ID")?;
    let endpoint = docker::command(&[
        "context".into(),
        "inspect".into(),
        "--format".into(),
        "{{.Endpoints.docker.Host}}".into(),
    ])
    .await?;
    anyhow::ensure!(
        endpoint.status.success(),
        "local Docker endpoint unavailable"
    );
    let endpoint = String::from_utf8(endpoint.stdout)?;
    let endpoint = endpoint.trim();
    anyhow::ensure!(
        endpoint.starts_with("unix:///")
            || (endpoint.starts_with("npipe://")
                && endpoint.ends_with("/pipe/dockerDesktopLinuxEngine")),
        "owner contract requires a local Docker engine"
    );

    let container = docker::GuardedContainer::registered()?;
    let capture = logs::capture()?;
    let image = docker::command(&[
        "image".into(),
        "inspect".into(),
        "postgres:18".into(),
        "--format".into(),
        "{{.Id}}".into(),
    ])
    .await?;
    anyhow::ensure!(image.status.success(), "local PostgreSQL18 image required");
    let image = String::from_utf8(image.stdout)?.trim().to_string();
    anyhow::ensure!(
        image.starts_with("sha256:") && image.len() == 71,
        "immutable PostgreSQL fixture required"
    );
    let database = format!("duskcue_owner_test_{}", resource.replace('-', ""));
    let password = duskcue::domains::auth::service::generate_session_token();
    let mut args = container.run_args();
    args.extend([
        "-d".into(),
        "-p".into(),
        "127.0.0.1::5432".into(),
        "-e".into(),
        format!("POSTGRES_PASSWORD={password}"),
        "-e".into(),
        format!("POSTGRES_DB={database}"),
        image.clone(),
        "postgres".into(),
        "-c".into(),
        "shared_buffers=16MB".into(),
        "-c".into(),
        "max_connections=16".into(),
    ]);
    let created = docker::command(&args).await?;
    anyhow::ensure!(created.status.success(), "owned PostgreSQL start failed");
    let outcome=async {
        let port=docker::command(&["port".into(),container.name.clone(),"5432/tcp".into()]).await?;
        anyhow::ensure!(port.status.success(),"owned fixture port unavailable");
        let port=String::from_utf8(port.stdout)?.trim().strip_prefix("127.0.0.1:").ok_or_else(||anyhow::anyhow!("nonlocal fixture port"))?.parse::<u16>()?;
        let url=format!("postgres://postgres:{password}@127.0.0.1:{port}/{database}");
        let deadline=Instant::now()+Duration::from_secs(30);
        let pool=loop {
            match PgPoolOptions::new().max_connections(4).acquire_timeout(Duration::from_secs(2)).connect(&url).await {
                Ok(pool)=>break pool,
                Err(_) if Instant::now()<deadline=>tokio::time::sleep(Duration::from_millis(200)).await,
                Err(_)=>anyhow::bail!("owned PostgreSQL readiness failed"),
            }
        };
        let result=async {
            sqlx::migrate!("./migrations").run(&pool).await?;
            sqlx::query("CREATE TABLE audit_owner_test PARTITION OF audit_log DEFAULT").execute(&pool).await?;
            let root=std::env::temp_dir().join(format!("duskcue-owner-{resource}"));
            let bootstrap=BootstrapConfig {database_url:None,bind_address:"127.0.0.1".into(),port:0,data_dir:root.join("data"),cache_dir:root.join("cache"),log_level:"error".into(),environment:"test".into(),encryption_key:None,geoip_license_key:None};
            let recorder=metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
            let (key,_)=EncryptionKey::generate();
            let state=AppState::new(pool.clone(),bootstrap,recorder.handle(),key);
            let app=duskcue::router::build_router(state.clone()).with_state(state.clone());
            let mut secrets=vec![password.clone(),http::PASSWORD.to_string()];
            let cli_actor=http::actor(&pool,"standard",false).await?;
            secrets.push(cli_actor.token.clone());
            operator_cases::run(&url,&root,&pool,cli_actor.user,&mut secrets).await?;
            password_cases::run(&app,&state,&pool,&mut secrets).await?;
            println!("OWNER_SECURITY_STAGE password_contract_passed");
            legacy_cases::run(&app,&pool,&mut secrets).await?;
            passkey_cases::run(&app,&state,&pool,&mut secrets).await?;
            println!("OWNER_SECURITY_STAGE passkey_contract_passed");
            let events:Vec<serde_json::Value>=sqlx::query_scalar("SELECT new_data FROM audit_log WHERE table_name='household_owner_state'").fetch_all(&pool).await?;
            anyhow::ensure!(events.len()>20 && events.iter().all(|e|e.as_object().is_some_and(|o|o.len()==1 && o.contains_key("event"))),"safe security audit events missing");
            while state.rate_limits.ip_global.check_key(&std::net::IpAddr::from([0,0,0,1])).is_ok() {}
            anyhow::ensure!(http::call(&app,Some(&cli_actor),"GET",&format!("{}/status",http::ROOT),None).await?.0==axum::http::StatusCode::TOO_MANY_REQUESTS,"outer rate-limit response was not exercised");
            let refs=secrets.iter().map(String::as_str).collect::<Vec<_>>();capture.check(&refs)?;
            println!("OWNER_SECURITY_RESULT {}",json!({"version":1,"resource_id":resource,"postgres_image":image,"password":true,"signed_webauthn":true,"cookie_and_bearer_rotation":true,"operator_command":true,"outer_rate_limit_no_store":true,"stable_session_bindings":true,"legacy_preferences_and_kids_pin":true,"factor_capacity_and_recovery":true,"protected_mutations":true,"recovery":true,"replay_refusal":true,"revocation_race":true,"safe_audit_events":events.len()}));
            Ok::<_,anyhow::Error>(())
        }.await;
        pool.close().await;
        result
    }.await;
    container.remove().await?;
    outcome
}
