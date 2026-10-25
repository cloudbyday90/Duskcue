// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment,
    http::{self, Case},
    network_cases,
    peer::{Behavior, Peer, Security},
};
use axum::http::StatusCode;
use duskcue::domains::{
    auth::{service::update_capabilities, types::CapabilityOverride},
    live_tv::{assignment::secrets::FeedField, feed::FeedError},
};
use serde_json::json;
use sqlx::{Connection, Row};
use std::{
    net::IpAddr,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub async fn run(case: &mut Case, database_url: &str) -> anyhow::Result<()> {
    let peer = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    let value = assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    let id = Uuid::parse_str(
        value["assignment_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("assignment id missing"))?,
    )?;
    let origin = peer.origin("http", "duskcue-feed.invalid");
    let body = json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["127.0.0.1"]});
    for invalid in [
        json!({"operation":"approve_destination","feed":"playlist","expected_origin":"http://other.invalid","addresses":["127.0.0.1"]}),
        json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["169.254.169.254"]}),
        json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["127.0.0.1","::ffff:127.0.0.1"]}),
        json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":[]}),
        json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["localhost"]}),
    ] {
        assignment::patch(case, invalid)
            .await?
            .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    }
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &case.admin_path(),
        Some(body.clone()),
        &[],
    )
    .await?
    .expect(StatusCode::PRECONDITION_REQUIRED)?;
    let old_etag = case.admin_read().await?.etag()?;
    case.admin.cookie = true;
    assignment::patch(case, body.clone())
        .await?
        .expect(StatusCode::OK)?;
    http::call(
        &case.app,
        Some(&case.admin),
        "PATCH",
        &case.admin_path(),
        Some(body.clone()),
        &[("if-match", &old_etag)],
    )
    .await?
    .expect(StatusCode::PRECONDITION_FAILED)?;
    http::call(
        &case.app,
        Some(&case.owner),
        "PATCH",
        &case.admin_path(),
        Some(body.clone()),
        &[("if-match", &old_etag)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    let viewing = http::Actor {
        user: case.admin.user,
        profile: case.admin.profile,
        token: http::session(&case.pool, case.admin.user, case.admin.profile).await?,
        cookie: false,
    };
    case.secrets.push(viewing.token.clone());
    http::call(
        &case.app,
        Some(&viewing),
        "PATCH",
        &case.admin_path(),
        Some(body.clone()),
        &[("if-match", &old_etag)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;

    case.owner_proof(true).await?;
    let generation = case.owner_read().await?.value["source_generation"].clone();
    case.owner_change(
        json!({"operation":"set_profile_access","profile_id":case.standard,"enabled":true}),
    )
    .await?
    .expect(StatusCode::OK)?;
    case.owner_change(json!({"operation":"set_live_limit","max_live_streams":2}))
        .await?
        .expect(StatusCode::OK)?;
    case.owner_change(
        json!({"operation":"set_profile_access","profile_id":case.kids,"enabled":true}),
    )
    .await?
    .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    anyhow::ensure!(
        case.owner_read().await?.value["source_generation"] == generation,
        "viewing policy changed source authority"
    );
    assignment::patch(case, body.clone())
        .await?
        .expect(StatusCode::OK)?;
    http::call(&case.app, Some(&case.owner), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;

    let before = case.admin_read().await?;
    let url = format!("{origin}/rotated?credential=feed-query-private");
    assignment::patch(
        case,
        json!({"operation":"rotate_credentials","playlist_url":url,"guide_url":url}),
    )
    .await?
    .expect(StatusCode::OK)?;
    let rotated = case.admin_read().await?;
    anyhow::ensure!(
        rotated.value["assignment_id"] == before.value["assignment_id"]
            && rotated.value["playlist_destination_approved"] == true
            && rotated.value["source_generation"].as_i64()
                > before.value["source_generation"].as_i64(),
        "same-origin rotation lost approval or failed fencing"
    );
    assignment::patch(case,json!({"operation":"approve_destination","feed":"guide","expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(StatusCode::OK)?;
    assignment::patch(case,json!({"operation":"rotate_credentials","playlist_url":"http://changed.invalid/private?credential=feed-query-private","guide_url":url})).await?.expect(StatusCode::OK)?;
    let changed = case.admin_read().await?;
    anyhow::ensure!(
        changed.value["playlist_destination_approved"] == false
            && changed.value["guide_destination_approved"] == true,
        "origin change did not independently clear approval"
    );
    assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    assignment::patch(
        case,
        json!({"operation":"revoke_destination","feed":"guide"}),
    )
    .await?
    .expect(StatusCode::OK)?;

    let row = sqlx::query("SELECT playlist_destination FROM live_tv_assignments WHERE id=$1")
        .bind(id)
        .fetch_one(&case.pool)
        .await?;
    let envelope: String = row.get("playlist_destination");
    case.secrets.push(envelope.clone());
    anyhow::ensure!(
        envelope.starts_with("duskcue-secret:v1:")
            && !envelope.contains(&origin)
            && !envelope.contains("127.0.0.1"),
        "destination was not encrypted"
    );
    let owner = network_cases::owner(case, true)?;
    anyhow::ensure!(
        owner
            .start(case.other.user, FeedField::Playlist, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "foreign household accessed assignment"
    );
    anyhow::ensure!(
        owner
            .start(case.owner.user, FeedField::Guide, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "unapproved guide inherited playlist authority"
    );
    anyhow::ensure!(
        owner
            .start(case.owner.user, FeedField::Playlist, 1)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "out-of-range pin was accepted"
    );
    sqlx::query("UPDATE live_tv_assignments SET guide_destination=playlist_destination,guide_address_count=playlist_address_count WHERE id=$1").bind(id).execute(&case.pool).await?;
    anyhow::ensure!(
        owner
            .start(case.owner.user, FeedField::Guide, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "approval accepted a different feed context"
    );
    sqlx::query("UPDATE live_tv_assignments SET guide_destination=NULL,guide_address_count=0,playlist_address_count=2 WHERE id=$1").bind(id).execute(&case.pool).await?;
    anyhow::ensure!(
        owner
            .start(case.owner.user, FeedField::Playlist, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "inconsistent approval metadata was accepted"
    );
    sqlx::query("UPDATE live_tv_assignments SET playlist_address_count=1 WHERE id=$1")
        .bind(id)
        .execute(&case.pool)
        .await?;
    owner.shutdown().await?;

    super::storage_cases::run(case, &peer).await?;
    approval_permission_race(case, database_url, body).await?;
    let before = case.admin_read().await?;
    sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
        .bind(case.owner.user)
        .execute(&case.pool)
        .await?;
    let owner = network_cases::owner(case, true)?;
    anyhow::ensure!(
        owner
            .start(case.owner.user, FeedField::Playlist, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "disabled household admitted networking"
    );
    sqlx::query("UPDATE users SET status='active' WHERE id=$1")
        .bind(case.owner.user)
        .execute(&case.pool)
        .await?;
    anyhow::ensure!(
        case.admin_read().await?.value["source_generation"].as_i64()
            == before.value["source_generation"].as_i64().map(|v| v + 2),
        "disable/re-enable did not advance generation twice"
    );
    owner.shutdown().await?;
    assignment::patch(
        case,
        json!({"operation":"replace_assignment","playlist_url":url,"guide_url":url}),
    )
    .await?
    .expect(StatusCode::OK)?;
    let replacement = case.admin_read().await?;
    anyhow::ensure!(
        replacement.value["assignment_id"] != before.value["assignment_id"]
            && replacement.value["playlist_address_count"] == 0
            && replacement.value["guide_address_count"] == 0
            && replacement.value["owner_max_live_streams"].is_null(),
        "replacement retained old authority or owner overrides"
    );
    case.configure(true, None, true, true).await?;
    anyhow::ensure!(
        peer.hits() == 0,
        "management operation contacted an upstream"
    );
    peer.finish(false).await
}

async fn approval_permission_race(
    case: &mut Case,
    database_url: &str,
    body: serde_json::Value,
) -> anyhow::Result<()> {
    let etag = case.admin_read().await?.etag()?;
    let mut coordinator = sqlx::PgConnection::connect(database_url).await?;
    let mut control = coordinator.begin().await?;
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
        http::call(
            &app,
            Some(&actor),
            "PATCH",
            &path,
            Some(body),
            &[("if-match", &etag)],
        )
        .await
    });
    let observed = async {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
            )
            .bind(pid)
            .fetch_one(&case.pool)
            .await?;
            if waiting {
                return Ok::<(), anyhow::Error>(());
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "approval did not reach transaction lock"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
    .await;
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_manage_users',false) ON CONFLICT(user_id,capability) DO UPDATE SET is_granted=false").bind(case.admin.user).execute(&mut *control).await?;
    control.commit().await?;
    task.await??.expect(StatusCode::FORBIDDEN)?;
    observed?;
    update_capabilities(
        &case.pool,
        case.admin.user,
        vec![CapabilityOverride {
            capability: "can_manage_users".into(),
            is_granted: true,
        }],
        "admin",
    )
    .await?;
    coordinator.close().await?;
    Ok(())
}
