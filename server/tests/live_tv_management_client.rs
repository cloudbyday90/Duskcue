// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "support/management_binding.rs"]
mod binding;
#[path = "support/household_database.rs"]
mod database;
#[path = "support/guarded_docker.rs"]
mod docker;
#[path = "support/management_http.rs"]
mod http;

use axum::{Router, extract::State, routing::get};
use serde_json::json;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::process::Command;

#[derive(Clone)]
struct Feeds {
    malformed: Arc<AtomicBool>,
    requests: Arc<AtomicUsize>,
}

async fn playlist(State(feeds): State<Feeds>) -> String {
    feeds.requests.fetch_add(1, Ordering::SeqCst);
    if feeds.malformed.load(Ordering::SeqCst) {
        "upstream error".into()
    } else {
        "#EXTM3U\n#EXTINF:-1 tvg-id=\"fixture\" tvg-name=\"Qualification channel\",Qualification channel\nhttp://127.0.0.1:9/proxy/ts/stream/019a4567-1234-7000-8000-000000000111\n".into()
    }
}
async fn guide(State(feeds): State<Feeds>) -> &'static str {
    feeds.requests.fetch_add(1, Ordering::SeqCst);
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?><tv><channel id=\"fixture\"><display-name>Qualification channel</display-name></channel><programme channel=\"fixture\" start=\"20261026140000 +0000\" stop=\"20261026150000 +0000\"><title lang=\"en\">Qualification programme</title></programme></tv>"
}
async fn invalidate(State(feeds): State<Feeds>) {
    feeds.malformed.store(true, Ordering::SeqCst);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires admitted disposable PostgreSQL18 and browser workflow"]
async fn real_management_client() -> anyhow::Result<()> {
    database::run(async |db| {
        anyhow::ensure!(!db.password.is_empty() && !db.url.is_empty(), "fixture identity missing");
        let state=db.state();
        let manager=duskcue::domains::live_tv::refresh::RefreshManager::attach(&state)?;
        let app=duskcue::router::build_router(state.clone()).with_state(state.clone());
        let outcome=async {
            binding::run(&app,&state).await?;
            let actor=http::actor(&db.pool,"standard",true).await?;
            sqlx::query("UPDATE users SET role='owner' WHERE id=$1").bind(actor.user).execute(&db.pool).await?;
            let target=http::actor(&db.pool,"standard",true).await?;
            let handoff=duskcue::domains::household_security::operator::issue_handoff(&db.pool,actor.user).await?;
            let feeds=Feeds{malformed:Arc::new(AtomicBool::new(false)),requests:Arc::new(AtomicUsize::new(0))};
            let feed_listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let origin=format!("http://{}",feed_listener.local_addr()?);
            let feed_stop=tokio_util::sync::CancellationToken::new();
            let feed_shutdown=feed_stop.clone();
            let feed_app=Router::new().route("/playlist",get(playlist)).route("/guide",get(guide)).route("/invalidate",axum::routing::post(invalidate)).with_state(feeds.clone());
            let feed_server=tokio::spawn(async move {axum::serve(feed_listener,feed_app).with_graceful_shutdown(feed_shutdown.cancelled_owned()).await});
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
            let address=listener.local_addr()?;
            let stop=tokio_util::sync::CancellationToken::new(); let shutdown=stop.clone();
            let server=tokio::spawn(async move {axum::serve(listener,app).with_graceful_shutdown(shutdown.cancelled_owned()).await});
            let fixture=db.root.join("management.json");
            tokio::fs::write(&fixture,serde_json::to_vec(&json!({"token":actor.token,"handoff":handoff,"password":http::PASSWORD,"target":target.user,"origin":origin,"user":{"id":actor.user,"username":format!("owner-fixture-{}",actor.user),"display_name":"Administrator","role":"owner","capabilities":[],"active_profile_id":actor.profile,"profile_selection_required":false}}))?).await?;
            let repo=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
            let mut command=Command::new("node");
            command.current_dir(repo.join("clients/web")).args(["node_modules/@playwright/test/cli.js","test","--config","playwright.management.config.js","--workers=1"]).env("DUSKCUE_BACKEND_URL",format!("http://{address}")).env("DUSKCUE_MANAGEMENT_FIXTURE",&fixture).env("TOKIO_WORKER_THREADS","2").stdin(std::process::Stdio::null()).kill_on_drop(true);
            #[cfg(target_os="windows")]
            command.creation_flags(0x08000000);
            let result=tokio::time::timeout(Duration::from_secs(240),command.status()).await;
            stop.cancel();feed_stop.cancel();state.event_streams.close();
            tokio::time::timeout(Duration::from_secs(15),server).await???;
            tokio::time::timeout(Duration::from_secs(15),feed_server).await???;
            state.event_streams.drain().await?;
            anyhow::ensure!(result??.success(),"real management browser qualification failed");
            anyhow::ensure!(feeds.requests.load(Ordering::SeqCst)==4,"unexpected upstream fetch or replay");
            println!("MANAGEMENT_CLIENT_RESULT {}",json!({"resource_id":db.resource,"postgres_image":db.image,"real_postgres":true,"normal_trust":true,"upstream_requests":4,"atomic_last_good":true,"bound_routes":true}));
            Ok(())
        }.await;
        manager.shutdown().await?;
        outcome
    }).await
}
