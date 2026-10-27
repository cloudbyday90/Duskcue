// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{commit_proxy::Proxy, database::Database, support::Fixture};
use anyhow::Context;
use reqwest::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn wire(
    db: &Database,
    needle: &'static [u8],
    before: bool,
    begin: bool,
) -> anyhow::Result<()> {
    let proxy = Proxy::start(&db.url, needle, before, begin).await?;
    proxy.resume();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&proxy.url)
        .await?;
    let mut f = Fixture::with_owner_pool(db, Some(pool.clone())).await?;
    let release = needle == b"SET state='released'";
    let admission = needle == b"INSERT INTO playback_vod_commands" || begin;
    let phase = if release {
        "release"
    } else if admission {
        "admission"
    } else {
        "readiness"
    };
    let result = async {
        let id = Uuid::now_v7();
        if release {
            f.start(id, true, false).await?;
            f.ready(id).await?;
        }
        proxy.arm();
        let response = if release {
            f.json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/stop"),
                Some(json!({})),
            )
            .await?
        } else {
            f.json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(id, true, false)),
            )
            .await?
        };
        anyhow::ensure!(matches!(
            response.0,
            StatusCode::ACCEPTED | StatusCode::SERVICE_UNAVAILABLE
        ));
        if !admission && !release {
            anyhow::ensure!(response.0==StatusCode::ACCEPTED,"readiness fault requires accepted preparation: {}",response.1);
        }
        let fault = tokio::time::timeout(Duration::from_secs(20), async {
            while proxy.dropped() == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await;
        if fault.is_err() {
            let evidence:Option<(String,String,bool,bool)> = tokio::time::timeout(Duration::from_secs(3),sqlx::query_as("SELECT r.state,c.state,c.resources_completed,c.history_id IS NOT NULL FROM playback_reservations r JOIN playback_vod_commands c ON c.reservation_id=r.id WHERE r.id=$1").bind(id).fetch_optional(&db.pool)).await.ok().and_then(Result::ok).flatten();
            anyhow::bail!("wire fault not reached: phase={phase} before_commit={before} response={} state={evidence:?}",response.0);
        }
        proxy.resume();
        if !release {
            let observed = f
                .json(
                    Method::GET,
                    &format!("/api/v1/playback-commands/{id}"),
                    None,
                )
                .await?;
            anyhow::ensure!(observed.1.get("stream_url").is_none_or(|v| v.is_null()));
        }
        let history: i64 = sqlx::query_scalar("SELECT count(*) FROM play_sessions")
            .fetch_one(&db.pool)
            .await?;
        if begin || needle == b"INSERT INTO playback_vod_commands" {
            anyhow::ensure!(history == 0);
        }
        if !release && !before && needle == b"UPDATE playback_vod_commands SET state='ready'" {
            anyhow::ensure!(history == 1);
        }
        f.owner.stop(id)?;
        if let Some(viewer) = f.base.state.vod_commands.get(id) {
            viewer.retry.notify_one();
        }
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let held: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM playback_reservations WHERE state<>'released'",
                )
                .fetch_one(&db.pool)
                .await?;
                if held == 0 {
                    return Ok::<_, anyhow::Error>(());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await.with_context(||format!("wire cleanup not released: phase={phase} before_commit={before}"))??;
        anyhow::ensure!(f.base.state.vod_commands.get(id).is_none());
        Ok::<_, anyhow::Error>(())
    }
    .await;
    proxy.resume();
    let cleanup = f.close().await;
    pool.close().await;
    let transport = proxy.finish().await;
    result?;
    if release {
        anyhow::ensure!(
            cleanup.is_err(),
            "release acknowledgement loss must retain root failure"
        );
    } else {
        cleanup?;
    }
    transport
}

pub(super) async fn completion_loss(db: &Database) -> anyhow::Result<()> {
    let mut f = Fixture::new(db).await?;
    let result = f
        .owner
        .sql::<(), _>(false, |_| {
            Box::pin(async { panic!("controlled owned SQL completion loss") })
        })
        .await;
    let cleanup = f.close().await;
    anyhow::ensure!(result.is_err() && cleanup.is_err());
    Ok(())
}
