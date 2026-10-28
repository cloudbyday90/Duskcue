// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{commit_proxy::Proxy, database::Database, support::Fixture};
use reqwest::Method;
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn wire(db: &Database, phase: u8, before: bool) -> anyhow::Result<()> {
    let needle: &'static [u8] = match phase {
        0 => b"INSERT INTO playback_start_intents",
        1 => b"UPDATE playback_start_intents SET preparation_complete",
        _ => b"UPDATE playback_start_intents SET admission_fenced=true",
    };
    let proxy = Proxy::start(&db.url, needle, before, false).await?;
    proxy.resume();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&proxy.url)
        .await?;
    let mut f = Fixture::with_owner_pool(db, Some(pool.clone())).await?;
    let result = async {
        let id = Uuid::now_v7();
        proxy.arm();
        let response = if phase == 2 {
            f.json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/withdraw"),
                Some(json!({"protocol":"duskcue-vod-command/2"})),
            )
            .await?
        } else {
            f.json(
                Method::POST,
                "/api/v1/playback-commands",
                Some(f.start_body(id, false, false)),
            )
            .await?
        };
        anyhow::ensure!(
            response.0 == 503,
            "an unknown commit must not acknowledge acceptance or settlement: {response:?}"
        );
        anyhow::ensure!(proxy.dropped() == 1);
        proxy.resume();
        let saved: Option<(bool, bool)> = sqlx::query_as(
            "SELECT admission_fenced,preparation_complete FROM playback_start_intents WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
        if phase == 0 {
            anyhow::ensure!(saved == if before { None } else { Some((false, false)) });
        }
        if phase == 1 {
            anyhow::ensure!(saved == Some((false, !before)));
        }
        if phase == 2 {
            anyhow::ensure!(saved == if before { None } else { Some((true, true)) });
        }
        let reservations: i64 =
            sqlx::query_scalar("SELECT count(*) FROM playback_reservations WHERE id=$1")
                .bind(id)
                .fetch_one(&db.pool)
                .await?;
        anyhow::ensure!(
            reservations == 0,
            "indeterminate pre-admission work must not launch a viewer"
        );
        let (_, observed) = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}/settlement"),
                None,
            )
            .await?;
        anyhow::ensure!((observed["status"] == "settled") == (phase == 2 && !before));
        let (_, withdrew) = f
            .json(
                Method::POST,
                &format!("/api/v1/playback-commands/{id}/withdraw"),
                Some(json!({"protocol":"duskcue-vod-command/2"})),
            )
            .await?;
        if saved.is_some_and(|(_, complete)| !complete) {
            anyhow::ensure!(
                withdrew["status"] != "settled" && withdrew["preparation_complete"] == false
            );
            let revision = crate::services::playback_admission::operator::fence(
                &db.pool,
                f.owner.instance(),
                1,
            )
            .await?;
            crate::services::playback_admission::operator::reconcile(
                &db.pool,
                f.owner.instance(),
                revision,
                true,
            )
            .await?;
        }
        let (_, settled) = f
            .json(
                Method::GET,
                &format!("/api/v1/playback-commands/{id}/settlement"),
                None,
            )
            .await?;
        anyhow::ensure!(settled["status"] == "settled");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    proxy.resume();
    let cleanup = f.close().await;
    pool.close().await;
    let closed = proxy.finish().await;
    result?;
    if phase < 2 {
        anyhow::ensure!(
            cleanup.is_err(),
            "unknown preparation ownership must retain root failure"
        );
    } else {
        cleanup?;
    }
    closed
}
