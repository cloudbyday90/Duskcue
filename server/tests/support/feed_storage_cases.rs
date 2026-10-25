// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment,
    http::{self, Case},
    network_cases,
    peer::Peer,
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::{assignment::secrets::FeedField, feed::FeedError};
use serde_json::json;

pub async fn run(case: &mut Case, peer: &Peer) -> anyhow::Result<()> {
    let origin = peer.origin("http", "duskcue-feed.invalid");
    let url = format!("{origin}/private-feed?credential=feed-query-private");
    let other_path = format!("/api/v1/live-tv/households/{}/assignment", case.other.user);
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &other_path,
        Some(json!({"playlist_url":url,"guide_url":url,"enabled":true})),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CREATED)?;
    sqlx::query("UPDATE live_tv_assignments SET playlist_destination=(SELECT playlist_destination FROM live_tv_assignments WHERE household_user_id=$2),playlist_address_count=1 WHERE household_user_id=$1").bind(case.other.user).bind(case.owner.user).execute(&case.pool).await?;
    let owner = network_cases::owner(case, true)?;
    anyhow::ensure!(
        owner
            .start(case.other.user, FeedField::Playlist, 0)?
            .finish()
            .await
            == Err(FeedError::Authority),
        "copied household/assignment approval was accepted"
    );
    owner.shutdown().await?;
    let etag = http::call(&case.app, Some(&case.admin), "GET", &other_path, None, &[])
        .await?
        .expect(StatusCode::OK)?
        .etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &other_path,
        None,
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;

    sqlx::query(
        "UPDATE live_tv_assignments SET playlist_secret=guide_secret WHERE household_user_id=$1",
    )
    .bind(case.owner.user)
    .execute(&case.pool)
    .await?;
    assignment::patch(
        case,
        json!({"operation":"rotate_credentials","playlist_url":url,"guide_url":url}),
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        case.admin_read().await?.value["playlist_destination_approved"] == false,
        "repair rotation retained unverifiable approval"
    );
    let approve = json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["127.0.0.1"]});
    assignment::patch(case, approve.clone())
        .await?
        .expect(StatusCode::OK)?;

    let before = case.admin_read().await?;
    let generation = before.value["source_generation"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("generation absent"))?;
    sqlx::query("UPDATE live_tv_assignments SET source_generation=$2 WHERE household_user_id=$1")
        .bind(case.owner.user)
        .bind(i64::MAX)
        .execute(&case.pool)
        .await?;
    assignment::patch(
        case,
        json!({"operation":"revoke_destination","feed":"playlist"}),
    )
    .await?
    .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let refused = case.admin_read().await?;
    anyhow::ensure!(
        refused.value["playlist_destination_approved"] == true
            && refused.value["revision"] == before.value["revision"],
        "generation overflow partially changed approval"
    );
    anyhow::ensure!(
        sqlx::query("UPDATE users SET status='disabled' WHERE id=$1")
            .bind(case.owner.user)
            .execute(&case.pool)
            .await
            .is_err(),
        "account mutation accepted generation overflow"
    );
    sqlx::query("UPDATE live_tv_assignments SET source_generation=$2 WHERE household_user_id=$1")
        .bind(case.owner.user)
        .bind(generation)
        .execute(&case.pool)
        .await?;

    let etag = case.admin_read().await?.etag()?;
    let path = case.admin_path();
    let conditions = [("if-match", etag.as_str())];
    let (a, b) = tokio::join!(
        http::call(
            &case.app,
            Some(&case.admin),
            "PATCH",
            &path,
            Some(approve.clone()),
            &conditions
        ),
        http::call(
            &case.app,
            Some(&case.admin),
            "PATCH",
            &path,
            Some(approve.clone()),
            &conditions
        )
    );
    let (a, b) = (a?.status, b?.status);
    anyhow::ensure!(
        matches!(
            (a, b),
            (StatusCode::OK, StatusCode::PRECONDITION_FAILED)
                | (StatusCode::PRECONDITION_FAILED, StatusCode::OK)
        ),
        "concurrent conditional writes did not select exactly one winner"
    );
    anyhow::ensure!(
        case.admin_read().await?.value["source_generation"].as_i64() == Some(generation + 1),
        "concurrent conditional writes incremented generation more than once"
    );
    Ok(())
}
