// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::{self, Case},
    import_fixture as fixture, refresh_cases,
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::refresh::RefreshManager;
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

pub async fn run(case: &mut Case, url: &str) -> anyhow::Result<()> {
    for (before_commit, claim_ack) in [(false, true), (true, true), (false, false)] {
        case.admin_proof(false).await?;
        refresh_cases::enable(case, true).await?;
        let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
        fixture::prepare(case, &a, &b).await?;
        let prior = fixture::status(case).await?.snapshot_id;
        let proxy = super::commit_proxy::Proxy::start(url, before_commit, claim_ack).await?;
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&proxy.url)
            .await?;
        let mut state = case.state.clone();
        state.pool = pool.clone();
        state.live_tv_refresh = Arc::new(OnceLock::new());
        let owner = RefreshManager::qualification_fixture(&state)?;
        let app = duskcue::router::build_router(state.clone()).with_state(state);
        let path = refresh_cases::path(case);
        let tag = http::call(&app, Some(&case.admin), "GET", &path, None, &[])
            .await?
            .etag()?;
        let reply = http::call(
            &app,
            Some(&case.admin),
            "POST",
            &path,
            Some(json!({"playlist_pin":0,"guide_pin":0})),
            &[("if-match", &tag)],
        )
        .await?;
        reply.expect(if before_commit {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::ACCEPTED
        })?;
        tokio::time::timeout(Duration::from_secs(10), async {
            while !owner.idle() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await?;
        anyhow::ensure!(
            proxy.dropped() == 1,
            "manual-refresh commit fault did not occur exactly once"
        );
        let current = fixture::status(case).await?;
        if before_commit {
            anyhow::ensure!(
                a.hits() == 0 && b.hits() == 0 && current.snapshot_id == prior,
                "uncertain unaccepted admission fetched or replayed work"
            );
        } else {
            anyhow::ensure!(
                a.hits() == 1
                    && b.hits() == 1
                    && current.state == "succeeded"
                    && current.snapshot_id != prior,
                "accepted commit acknowledgment loss did not reconcile exactly once"
            );
        }
        owner.shutdown().await?;
        pool.close().await;
        a.finish(!before_commit).await?;
        b.finish(!before_commit).await?;
        proxy.finish().await?;
    }
    println!("IPTV_REFRESH_STAGE acceptance_and_publication_commit_faults_passed");
    Ok(())
}
