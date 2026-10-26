// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/household_actor.rs"]
mod actors;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;

use serde_json::json;
use std::{sync::Arc, time::Duration};
use tokio::process::Command;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 and browser workflow"]
async fn real_household_client() -> anyhow::Result<()> {
    database::run(async |db| {
        let actor = actors::actor(&db.pool, "standard", true).await?;
        anyhow::ensure!(!actor.cookie && !db.password.is_empty(), "owned fixture identity missing");
        let handoff = duskcue::domains::household_security::operator::issue_handoff(&db.pool, actor.user).await?;
        let mut state = db.state();
        state.webauthn = Arc::new(webauthn_rs::WebauthnBuilder::new("localhost", &url::Url::parse("http://localhost:48037")?)?.rp_name("Duskcue qualification").build()?);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let stop = tokio_util::sync::CancellationToken::new();
        let shutdown = stop.clone();
        let app = duskcue::router::build_router(state.clone()).with_state(state.clone());
        let server = tokio::spawn(async move { axum::serve(listener, app).with_graceful_shutdown(shutdown.cancelled_owned()).await });
        let fixture = db.root.join("browser.json");
        tokio::fs::write(&fixture, serde_json::to_vec(&json!({
            "token": actor.token, "handoff": handoff, "password": actors::PASSWORD,
            "databaseUrl": db.url, "operator": env!("CARGO_BIN_EXE_duskcue"), "root": db.root,
            "user": {"id": actor.user, "username": format!("owner-fixture-{}",actor.user), "display_name":"Owner security fixture", "role":"member", "capabilities":[], "active_profile_id":actor.profile,"profile_selection_required":false}
        }))?).await?;
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut command = Command::new("node");
        command.current_dir(repo.join("clients/web"))
            .args(["node_modules/@playwright/test/cli.js", "test", "--config", "playwright.owner.config.js", "--workers=1"])
            .env("DUSKCUE_BACKEND_URL", format!("http://{address}"))
            .env("DUSKCUE_OWNER_FIXTURE", &fixture)
            .env("TOKIO_WORKER_THREADS", "2")
            .stdin(std::process::Stdio::null()).kill_on_drop(true);
        #[cfg(target_os = "windows")]
        command.creation_flags(0x08000000);
        let result = tokio::time::timeout(Duration::from_secs(240), command.status()).await;
        stop.cancel();
        state.event_streams.close();
        let joined = tokio::time::timeout(Duration::from_secs(15), server).await;
        joined???;
        state.event_streams.drain().await?;
        anyhow::ensure!(result??.success(), "real owner browser qualification failed");
        let trusted: i64 = sqlx::query_scalar("SELECT count(*) FROM user_passkeys WHERE user_id=$1 AND owner_trusted").bind(actor.user).fetch_one(&db.pool).await?;
        anyhow::ensure!(trusted == 1, "replacement passkey was not persisted and trusted");
        println!("OWNER_CLIENT_RESULT {}", json!({"resource_id":db.resource,"postgres_image":db.image,"real_postgres":true,"real_browser_webauthn":true,"replacement_recovery":true,"cookie_rotation":true,"trusted_passkeys":trusted}));
        Ok(())
    }).await
}
