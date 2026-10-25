// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    http::{self, Case},
    import_fixture as fixture, refresh_cases,
};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::json;
use sqlx::{Connection, PgConnection};
use std::time::Duration;

pub async fn run(case: &mut Case, url: &str) -> anyhow::Result<()> {
    case.admin_proof(false).await?;
    refresh_cases::enable(case, true).await?;
    let (a, b) = fixture::peers(&fixture::playlist(), &fixture::guide()).await?;
    fixture::prepare(case, &a, &b).await?;
    let path = refresh_cases::path(case);
    let tag = http::call(&case.app, Some(&case.admin), "GET", &path, None, &[])
        .await?
        .etag()?;
    let simple = Request::builder()
        .method("POST")
        .uri(&path)
        .header("authorization", format!("Bearer {}", case.admin.token))
        .header("content-type", "text/plain")
        .header("if-match", &tag)
        .body(Body::from("{}"))?;
    http::raw(&case.app, simple)
        .await?
        .expect(StatusCode::UNSUPPORTED_MEDIA_TYPE)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "POST",
        &path,
        Some(json!({"playlist_pin":0,"guide_pin":0,"extra":"x".repeat(17000)})),
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::PAYLOAD_TOO_LARGE)?;
    for change in ["capability", "condition", "disconnect"] {
        let tag = http::call(&case.app, Some(&case.admin), "GET", &path, None, &[])
            .await?
            .etag()?;
        let mut connection = PgConnection::connect(url).await?;
        let mut tx = connection.begin().await?;
        sqlx::query("SELECT id FROM users WHERE id=$1 FOR UPDATE")
            .bind(case.admin.user)
            .fetch_one(&mut *tx)
            .await?;
        let app = case.app.clone();
        let actor = http::Actor {
            user: case.admin.user,
            profile: case.admin.profile,
            token: case.admin.token.clone(),
            cookie: case.admin.cookie,
        };
        let target = path.clone();
        let task = tokio::spawn(async move {
            http::call(
                &app,
                Some(&actor),
                "POST",
                &target,
                Some(json!({"playlist_pin":0,"guide_pin":0})),
                &[("if-match", &tag)],
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(4),async {
            loop {
                let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE wait_event_type='Lock' AND query LIKE 'SELECT id FROM users WHERE id=ANY%')").fetch_one(&case.pool).await?;
                if waiting {return Ok::<_,anyhow::Error>(());}tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }).await??;
        if change == "disconnect" {
            task.abort();
            let _ = task.await;
            tx.commit().await?;
            refresh_cases::wait_idle(case.state.live_tv_refresh.get().unwrap()).await?;
            anyhow::ensure!(
                a.hits() == 0 && b.hits() == 0,
                "pre-admission disconnect fetched work"
            );
            continue;
        }
        if change == "capability" {
            sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_manage_users',false) ON CONFLICT (user_id,capability) DO UPDATE SET is_granted=false").bind(case.admin.user).execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE live_tv_refresh_control SET revision=revision+1 WHERE singleton")
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        task.await??.expect(if change == "capability" {
            StatusCode::FORBIDDEN
        } else {
            StatusCode::PRECONDITION_FAILED
        })?;
        anyhow::ensure!(
            a.hits() == 0 && b.hits() == 0,
            "rejected admission reached an upstream"
        );
        if change == "capability" {
            sqlx::query(
                "DELETE FROM user_capabilities WHERE user_id=$1 AND capability='can_manage_users'",
            )
            .bind(case.admin.user)
            .execute(&case.pool)
            .await?;
        }
    }
    a.finish(false).await?;
    b.finish(false).await?;
    println!("IPTV_REFRESH_STAGE observed_admission_races_passed");
    Ok(())
}
