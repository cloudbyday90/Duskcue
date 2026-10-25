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
use duskcue::domains::live_tv::{
    assignment::secrets::FeedField,
    feed::{FeedError, FeedOwner},
};
use serde_json::json;
use std::{net::IpAddr, time::Duration};

pub async fn run(case: &mut Case, database_url: &str) -> anyhow::Result<()> {
    for (security, behavior, phase) in [
        (Security::StalledTls, Behavior::HeadersStall, 2),
        (Security::Plain, Behavior::HeadersStall, 3),
        (Security::Plain, Behavior::BodyStall, 4),
        (Security::Tls, Behavior::BodyStall, 4),
    ] {
        let tls = !matches!(security, Security::Plain);
        let peer = Peer::new(IpAddr::from([127, 0, 0, 1]), security, behavior).await?;
        assignment::prepare(
            case,
            &peer,
            if tls { "https" } else { "http" },
            "duskcue-feed.invalid",
        )
        .await?;
        let owner = network_cases::owner(case, true)?;
        let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
        peer.wait_phase(phase).await?;
        fetch.stop().await?;
        anyhow::ensure!(owner.idle(), "Stop retained capacity");
        owner.shutdown().await?;
        anyhow::ensure!(
            matches!(
                owner.start(case.owner.user, FeedField::Playlist, 0),
                Err(FeedError::Cancelled)
            ),
            "shutdown retained admission"
        );
        peer.finish(true).await?;
    }

    for abandoned in 0..3 {
        let peer = Peer::new(
            IpAddr::from([127, 0, 0, 1]),
            Security::Plain,
            Behavior::HeadersStall,
        )
        .await?;
        assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
        let owner = network_cases::owner(case, true)?;
        let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
        peer.wait_phase(3).await?;
        match abandoned {
            0 => {
                drop(fetch);
                owner.shutdown().await?;
                anyhow::ensure!(owner.idle(), "abandoned handle leaked capacity");
            }
            1 => {
                let task = tokio::spawn(fetch.finish());
                task.abort();
                anyhow::ensure!(
                    task.await.is_err_and(|e| e.is_cancelled()),
                    "caller task did not abort"
                );
                owner.shutdown().await?;
                anyhow::ensure!(owner.idle(), "abandoned caller leaked capacity");
            }
            _ => {
                drop(owner);
                anyhow::ensure!(
                    fetch.finish().await == Err(FeedError::Cancelled),
                    "owner drop did not cancel operation"
                );
            }
        }
        peer.finish(true).await?;
    }

    for mutation in [
        "revoke", "rotate", "replace", "account", "deleted", "disable",
    ] {
        case.configure(true, None, true, true).await?;
        let peer = Peer::new(
            IpAddr::from([127, 0, 0, 1]),
            Security::Plain,
            Behavior::BodyStall,
        )
        .await?;
        assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
        let owner = network_cases::owner(case, true)?;
        let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
        peer.wait_phase(4).await?;
        match mutation {
            "revoke" => {
                assignment::patch(
                    case,
                    json!({"operation":"revoke_destination","feed":"playlist"}),
                )
                .await?
                .expect(StatusCode::OK)?;
            }
            "rotate" | "replace" => {
                let url = format!(
                    "{}/new-secret?credential=feed-query-private",
                    peer.origin("http", "duskcue-feed.invalid")
                );
                assignment::patch(case,json!({"operation":if mutation=="rotate" {"rotate_credentials"} else {"replace_assignment"},"playlist_url":url,"guide_url":url})).await?.expect(StatusCode::OK)?;
            }
            "account" | "deleted" => {
                let mut tx = case.pool.begin().await?;
                let disable = if mutation == "account" {
                    "UPDATE users SET status='disabled' WHERE id=$1"
                } else {
                    "UPDATE users SET deleted_at=clock_timestamp() WHERE id=$1"
                };
                let enable = if mutation == "account" {
                    "UPDATE users SET status='active' WHERE id=$1"
                } else {
                    "UPDATE users SET deleted_at=NULL WHERE id=$1"
                };
                sqlx::query(disable)
                    .bind(case.owner.user)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(enable)
                    .bind(case.owner.user)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
            }
            _ => {
                case.configure(false, None, true, true).await?;
            }
        }
        anyhow::ensure!(
            fetch.finish().await == Err(FeedError::Authority),
            "stale operation returned a successful result"
        );
        owner.shutdown().await?;
        anyhow::ensure!(owner.idle(), "revoked operation retained capacity");
        peer.finish(true).await?;
    }
    case.configure(true, None, true, true).await?;
    capacity(case).await?;
    check_failure(case, database_url).await?;
    pre_connect_cancel(case, database_url).await?;
    assignment_deleted(case).await?;
    Ok(())
}

async fn pre_connect_cancel(case: &mut Case, database_url: &str) -> anyhow::Result<()> {
    let peer = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    let held = pool.acquire().await?;
    let owner = FeedOwner::qualification_fixture(pool.clone(), case.state.encryption_key.clone())?;
    let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
    tokio::time::sleep(Duration::from_millis(50)).await;
    fetch.stop().await?;
    owner.shutdown().await?;
    drop(held);
    pool.close().await;
    anyhow::ensure!(
        owner.idle() && peer.hits() == 0,
        "pre-connect cancellation leaked capacity or contacted a peer"
    );
    peer.finish(false).await
}

async fn assignment_deleted(case: &mut Case) -> anyhow::Result<()> {
    let peer = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::BodyStall,
    )
    .await?;
    assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    let owner = network_cases::owner(case, true)?;
    let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
    peer.wait_phase(4).await?;
    let etag = case.admin_read().await?.etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &case.admin_path(),
        None,
        &[("if-match", &etag)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;
    anyhow::ensure!(
        fetch.finish().await == Err(FeedError::Authority),
        "deleted assignment returned stale bytes"
    );
    owner.shutdown().await?;
    peer.finish(true).await
}

async fn capacity(case: &mut Case) -> anyhow::Result<()> {
    let first = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    let second = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::HeadersStall,
    )
    .await?;
    assignment::prepare(case, &first, "http", "duskcue-feed.invalid").await?;
    let url = format!(
        "{}/private-feed?credential=feed-query-private",
        second.origin("http", "duskcue-feed.invalid")
    );
    let path = format!("/api/v1/live-tv/households/{}/assignment", case.other.user);
    http::call(
        &case.app,
        Some(&case.admin),
        "PUT",
        &path,
        Some(json!({"playlist_url":url,"guide_url":url,"enabled":true})),
        &[("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CREATED)?;
    let etag = http::call(&case.app, Some(&case.admin), "GET", &path, None, &[])
        .await?
        .expect(StatusCode::OK)?
        .etag()?;
    http::call(&case.app,Some(&case.admin),"PATCH",&path,Some(json!({"operation":"approve_destination","feed":"playlist","expected_origin":second.origin("http","duskcue-feed.invalid"),"addresses":["127.0.0.1"]})),&[("if-match",&etag)]).await?.expect(StatusCode::OK)?;
    let owner = network_cases::owner(case, true)?;
    let a = owner.start(case.owner.user, FeedField::Playlist, 0)?;
    first.wait_phase(3).await?;
    anyhow::ensure!(
        matches!(
            owner.start(case.owner.user, FeedField::Guide, 0),
            Err(FeedError::Capacity)
        ),
        "same assignment admitted concurrent operation"
    );
    let b = owner.start(case.other.user, FeedField::Playlist, 0)?;
    second.wait_phase(3).await?;
    anyhow::ensure!(
        matches!(
            owner.start(case.admin.user, FeedField::Playlist, 0),
            Err(FeedError::Capacity)
        ),
        "owner exceeded total operation cap"
    );
    owner.shutdown().await?;
    anyhow::ensure!(
        a.finish().await == Err(FeedError::Cancelled)
            && b.finish().await == Err(FeedError::Cancelled)
            && owner.idle(),
        "shutdown did not join all admitted operations"
    );
    first.finish(true).await?;
    second.finish(true).await?;
    Ok(())
}

async fn check_failure(case: &mut Case, database_url: &str) -> anyhow::Result<()> {
    let peer = Peer::new(
        IpAddr::from([127, 0, 0, 1]),
        Security::Plain,
        Behavior::BodyStall,
    )
    .await?;
    assignment::prepare(case, &peer, "http", "duskcue-feed.invalid").await?;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await?;
    let owner = FeedOwner::qualification_fixture(pool.clone(), case.state.encryption_key.clone())?;
    let fetch = owner.start(case.owner.user, FeedField::Playlist, 0)?;
    peer.wait_phase(4).await?;
    let held = pool.acquire().await?;
    let result = tokio::time::timeout(Duration::from_secs(3), fetch.finish()).await;
    drop(held);
    owner.shutdown().await?;
    pool.close().await;
    anyhow::ensure!(
        result? == Err(FeedError::Authority),
        "authority-check timeout did not fail closed"
    );
    peer.finish(true).await
}
