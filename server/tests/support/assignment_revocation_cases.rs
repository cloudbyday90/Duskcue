// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::{self, Case};
use axum::http::StatusCode;
use duskcue::domains::{
    auth::{service::update_capabilities, types::CapabilityOverride},
    household_security::transactions,
};
use serde_json::json;
use sqlx::{Connection, PgPool, Row};
use std::time::{Duration, Instant};

async fn blocked(pool: &PgPool, blocker: i32) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observed: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
        )
        .bind(blocker)
        .fetch_one(pool)
        .await?;
        if observed {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "pending mutation did not reach database lock"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

pub async fn run(case: &mut Case, database_url: &str) -> anyhow::Result<()> {
    let mut coordinator = sqlx::PgConnection::connect(database_url).await?;
    let mut control = coordinator.begin().await?;
    let row =
        sqlx::query("SELECT id,pg_backend_pid() AS pid FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(case.admin.user)
            .fetch_one(&mut *control)
            .await?;
    let pid: i32 = row.get("pid");
    let pool = case.pool.clone();
    let user = case.admin.user;
    let capability = tokio::spawn(async move {
        update_capabilities(
            &pool,
            user,
            vec![CapabilityOverride {
                capability: "can_manage_users".into(),
                is_granted: false,
            }],
            "admin",
        )
        .await
    });
    let observed = blocked(&case.pool, pid).await;
    control.commit().await?;
    capability.await??;
    observed?;
    coordinator.close().await?;
    http::call(
        &case.app,
        Some(&case.admin),
        "GET",
        &case.admin_path(),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    update_capabilities(&case.pool, case.admin.user, vec![], "admin").await?;

    let before = case.admin_read().await?;
    let etag = before.etag()?;
    let mut control = case.pool.begin().await?;
    let pid: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(case.admin.user)
            .fetch_one(&mut *control)
            .await?;
    let app = case.app.clone();
    let path = case.admin_path();
    let actor = http::Actor {
        user: case.admin.user,
        profile: case.admin.profile,
        token: case.admin.token.clone(),
        cookie: case.admin.cookie,
    };
    let task = tokio::spawn(async move {
        http::call(&app,Some(&actor),"PATCH",&path,Some(json!({"operation":"configure","enabled":false,"max_live_streams":3,"allow_owner_profile_access":true,"allow_owner_live_limit":true})),&[("if-match",&etag)]).await
    });
    let observed = blocked(&case.pool, pid).await;
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_manage_users',false) ON CONFLICT(user_id,capability) DO UPDATE SET is_granted=false").bind(case.admin.user).execute(&mut *control).await?;
    control.commit().await?;
    task.await??.expect(StatusCode::FORBIDDEN)?;
    observed?;
    update_capabilities(&case.pool, case.admin.user, vec![], "admin").await?;
    anyhow::ensure!(
        case.admin_read().await?.value["revision"] == before.value["revision"],
        "stale administrator permission mutated assignment"
    );

    let owner_before = case.owner_read().await?;
    let etag = owner_before.etag()?;
    let mut control = case.pool.begin().await?;
    let pid: i32 =
        sqlx::query_scalar("SELECT pg_backend_pid() FROM users WHERE id=$1 FOR NO KEY UPDATE")
            .bind(case.owner.user)
            .fetch_one(&mut *control)
            .await?;
    let app = case.app.clone();
    let actor = http::Actor {
        user: case.owner.user,
        profile: case.owner.profile,
        token: case.owner.token.clone(),
        cookie: case.owner.cookie,
    };
    let task = tokio::spawn(async move {
        http::call(
            &app,
            Some(&actor),
            "PATCH",
            http::OWNER,
            Some(json!({"operation":"set_live_limit","max_live_streams":1})),
            &[("if-match", &etag)],
        )
        .await
    });
    let observed = blocked(&case.pool, pid).await;
    sqlx::query("UPDATE live_tv_assignments SET allow_owner_live_limit=false,revision=revision+1 WHERE household_user_id=$1").bind(case.owner.user).execute(&mut *control).await?;
    transactions::revoke_management(&mut control, case.owner.user).await?;
    control.commit().await?;
    task.await??.expect(StatusCode::FORBIDDEN)?;
    observed?;
    let saved: Option<i32> = sqlx::query_scalar(
        "SELECT owner_max_live_streams FROM live_tv_assignments WHERE household_user_id=$1",
    )
    .bind(case.owner.user)
    .fetch_one(&case.pool)
    .await?;
    anyhow::ensure!(
        saved == Some(2),
        "revoked pending owner mutation changed restriction"
    );
    case.configure(true, Some(3), true, true).await?;
    case.owner_proof(false).await?;
    Ok(())
}
