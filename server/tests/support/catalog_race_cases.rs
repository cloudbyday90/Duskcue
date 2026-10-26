// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{catalog_http as api, context_cases, fixture::Fixture, http};
use axum::http::StatusCode;
use sqlx::{Connection, PgConnection, PgPool};
use std::time::Duration;
use tokio::time::timeout;

fn actor(fixture: &Fixture) -> http::Actor {
    http::Actor {
        user: fixture.viewer.user,
        profile: fixture.case.standard,
        token: fixture.viewer.token.clone(),
        cookie: false,
    }
}
async fn waiting(pool: &PgPool, blocker: i32, prefix: &str) -> anyhow::Result<i32> {
    timeout(Duration::from_millis(450),async {loop {
        let pid:Option<i32>=sqlx::query_scalar("SELECT pid FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)) AND starts_with(query,$2) LIMIT 1").bind(blocker).bind(prefix).fetch_optional(pool).await?;
        if let Some(pid)=pid {return Ok::<_,anyhow::Error>(pid);}
        tokio::time::sleep(Duration::from_millis(5)).await;
    }}).await?
}
fn read(fixture: &Fixture) -> tokio::task::JoinHandle<anyhow::Result<api::Reply>> {
    let app = fixture.case.app.clone();
    let actor = actor(fixture);
    tokio::spawn(async move { api::call(&app, Some(&actor), api::ROOT).await })
}

pub async fn run(fixture: &mut Fixture, url: &str) -> anyhow::Result<()> {
    let mut coordinator = PgConnection::connect(url).await?;
    let session = context_cases::session(fixture).await?;
    for scenario in ["capability", "source", "inactive", "profile", "credential"] {
        let mut tx = coordinator.begin().await?;
        let account = matches!(scenario, "capability" | "source" | "inactive");
        let pid: i32 = if account {
            sqlx::query_scalar("SELECT pg_backend_pid() FROM users WHERE id=$1 FOR NO KEY UPDATE")
                .bind(fixture.viewer.user)
                .fetch_one(&mut *tx)
                .await?
        } else {
            sqlx::query_scalar(
                "SELECT pg_backend_pid() FROM user_profiles WHERE id=$1 FOR NO KEY UPDATE",
            )
            .bind(fixture.case.standard)
            .fetch_one(&mut *tx)
            .await?
        };
        let task = read(fixture);
        let observed = waiting(
            &fixture.case.pool,
            pid,
            if account {
                "SELECT role,status="
            } else {
                "SELECT profile_type="
            },
        )
        .await;
        let saved: Option<String> = if scenario == "source" {
            Some(sqlx::query_scalar("SELECT playlist_destination FROM live_tv_assignments WHERE household_user_id=$1").bind(fixture.viewer.user).fetch_one(&mut *tx).await?)
        } else {
            None
        };
        match scenario {
            "capability" => {
                sqlx::query("UPDATE user_capabilities SET is_granted=false WHERE user_id=$1 AND capability='can_use_live_tv'").bind(fixture.viewer.user).execute(&mut *tx).await?;
            }
            "source" => {
                sqlx::query("UPDATE live_tv_assignments SET playlist_destination=NULL,playlist_address_count=0 WHERE household_user_id=$1").bind(fixture.viewer.user).execute(&mut *tx).await?;
            }
            "profile" => {
                sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
                    .bind(session)
                    .bind(fixture.second)
                    .execute(&mut *tx)
                    .await?;
            }
            "inactive" => {
                sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
                    .bind(fixture.viewer.user)
                    .execute(&mut *tx)
                    .await?;
            }
            "credential" => {
                sqlx::query(
                    "UPDATE user_sessions SET credential_version=credential_version+1 WHERE id=$1",
                )
                .bind(session)
                .execute(&mut *tx)
                .await?;
            }
            _ => unreachable!(),
        }
        tx.commit().await?;
        let reply = timeout(Duration::from_secs(4), task).await???;
        match scenario {
            "capability" => {
                sqlx::query("UPDATE user_capabilities SET is_granted=true WHERE user_id=$1 AND capability='can_use_live_tv'").bind(fixture.viewer.user).execute(&fixture.case.pool).await?;
                reply.expect(StatusCode::FORBIDDEN)?;
            }
            "source" => {
                sqlx::query("UPDATE live_tv_assignments SET playlist_destination=$2,playlist_address_count=1 WHERE household_user_id=$1").bind(fixture.viewer.user).bind(saved).execute(&fixture.case.pool).await?;
                reply.expect(StatusCode::FORBIDDEN)?;
            }
            "profile" => {
                sqlx::query("UPDATE user_sessions SET active_profile_id=$2 WHERE id=$1")
                    .bind(session)
                    .bind(fixture.case.standard)
                    .execute(&fixture.case.pool)
                    .await?;
                reply.expect(StatusCode::CONFLICT)?;
            }
            "inactive" => {
                sqlx::query("UPDATE users SET status='active' WHERE id=$1")
                    .bind(fixture.viewer.user)
                    .execute(&fixture.case.pool)
                    .await?;
                reply.expect(StatusCode::FORBIDDEN)?;
                api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
                    .await?
                    .expect(StatusCode::SERVICE_UNAVAILABLE)?;
                fixture.snapshot = super::fixture::refresh(&fixture.case).await??;
            }
            "credential" => {
                reply.expect(StatusCode::UNAUTHORIZED)?;
            }
            _ => unreachable!(),
        }
        observed?;
        println!("IPTV_CATALOG_RACE {scenario}");
    }
    let mut tx = coordinator.begin().await?;
    let pid: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(fixture.viewer.user)
            .fetch_one(&mut *tx)
            .await?;
    let task = read(fixture);
    let observed = waiting(&fixture.case.pool, pid, "SELECT role,status=").await;
    task.abort();
    anyhow::ensure!(
        task.await.is_err_and(|e| e.is_cancelled()),
        "HTTP caller cancellation not observed"
    );
    timeout(Duration::from_secs(2), async {
        loop {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(&fixture.case.pool)
            .await?;
            if !exists {
                return Ok::<_, anyhow::Error>(());
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await??;
    tx.rollback().await?;
    observed?;
    let mut tx = coordinator.begin().await?;
    sqlx::query("SELECT id FROM users WHERE id=$1 FOR NO KEY UPDATE")
        .bind(fixture.viewer.user)
        .execute(&mut *tx)
        .await?;
    let began = tokio::time::Instant::now();
    let reply = read(fixture).await??;
    tx.rollback().await?;
    reply.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    anyhow::ensure!(
        began.elapsed() < Duration::from_secs(3),
        "lock failure expanded catalog budget"
    );
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    publication(fixture, &mut coordinator).await?;
    coordinator.close().await?;
    fixture.case.state.live_tv_catalog.close();
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    api::call(&fixture.case.app, None, api::ROOT)
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    println!("IPTV_CATALOG_STAGE observed_revocation_publication_cancellation_deadline_passed");
    Ok(())
}

async fn publication(fixture: &mut Fixture, coordinator: &mut PgConnection) -> anyhow::Result<()> {
    let prior = fixture.snapshot;
    let count = fixture.source.count();
    let gate = fixture.source.hold_guide().await;
    let owner = duskcue::domains::live_tv::feed::imports::ImportOwner::qualification_fixture(
        fixture.case.pool.clone(),
        fixture.case.state.encryption_key.clone(),
    )?;
    let run = owner.start(fixture.viewer.user, 0, 0)?;
    timeout(Duration::from_secs(3), async {
        while fixture.source.count() < count + 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    let mut tx = coordinator.begin().await?;
    let pid: i32 = sqlx::query_scalar(
        "SELECT pg_backend_pid() FROM user_profiles WHERE id=$1 FOR NO KEY UPDATE",
    )
    .bind(fixture.case.standard)
    .fetch_one(&mut *tx)
    .await?;
    let task = read(fixture);
    let reader = waiting(&fixture.case.pool, pid, "SELECT profile_type=").await?;
    gate.notify_one();
    let observed = waiting(
        &fixture.case.pool,
        reader,
        "SELECT id FROM users WHERE id=ANY",
    )
    .await;
    tx.commit().await?;
    let reply = task.await??.expect(StatusCode::OK)?;
    let published = run.finish().await;
    owner.shutdown().await?;
    let published = published?;
    observed?;
    anyhow::ensure!(
        reply.value["context"]["snapshot_id"] == prior.to_string() && published != prior,
        "publication mixed the retained read snapshot"
    );
    fixture.snapshot = published;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    Ok(())
}
