// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment, catalog_http as api,
    fixture::{self, Fixture},
    routes::{self, Inventory},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::feed::imports::{self, ImportError};
use serde_json::json;

pub async fn run(fixture: &mut Fixture, inventory: &Inventory) -> anyhow::Result<()> {
    let generation: i64 = sqlx::query_scalar(
        "SELECT source_generation FROM live_tv_assignments WHERE household_user_id=$1",
    )
    .bind(fixture.viewer.user)
    .fetch_one(&fixture.case.pool)
    .await?;
    let keys:Vec<Vec<u8>>=sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels WHERE snapshot_id=$1 ORDER BY channel_key").bind(fixture.snapshot).fetch_all(&fixture.case.pool).await?;
    let before = fixture.source.count();
    fixture
        .source
        .set(fixture::playlist(), b"<tv>".to_vec())
        .await?;
    anyhow::ensure!(
        fixture::refresh(&fixture.case).await? == Err(ImportError::Format),
        "same-generation malformed refresh was not refused"
    );
    anyhow::ensure!(
        fixture.source.count() == before + 2,
        "explicit refresh network accounting changed"
    );
    let reply = api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1", api::ROOT),
    )
    .await?
    .expect(StatusCode::OK)?;
    let snapshot_cursor = reply.value["cursor"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("snapshot fence cursor missing"))?
        .to_owned();
    anyhow::ensure!(
        reply.value["context"]["snapshot_id"] == fixture.snapshot.to_string()
            && reply.value["context"]["latest_refresh"]["state"] == "failed",
        "failed refresh lost last-good evidence"
    );
    api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &routes::programme_path(fixture.snapshot, inventory.channel, false, 20, None),
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        fixture.source.count() == before + 2,
        "last-good reads fetched source data"
    );
    let changed = String::from_utf8(fixture::playlist())?
        .replace("Channel-private", "Channel-new-private")
        .into_bytes();
    fixture.source.set(changed, fixture::guide()).await?;
    let second = fixture::refresh(&fixture.case).await??;
    anyhow::ensure!(
        second != fixture.snapshot,
        "successful refresh reused snapshot identity"
    );
    let now: i64 = sqlx::query_scalar(
        "SELECT source_generation FROM live_tv_assignments WHERE household_user_id=$1",
    )
    .bind(fixture.viewer.user)
    .fetch_one(&fixture.case.pool)
    .await?;
    anyhow::ensure!(
        generation == now,
        "same-source fixture unexpectedly advanced generation"
    );
    api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &format!("{}?limit=1&cursor={}", api::ROOT, snapshot_cursor),
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    api::call(
        &fixture.case.app,
        Some(&fixture.viewer),
        &routes::programme_path(fixture.snapshot, inventory.channel, false, 20, None),
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    let after:Vec<Vec<u8>>=sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels WHERE snapshot_id=$1 ORDER BY channel_key").bind(second).fetch_all(&fixture.case.pool).await?;
    anyhow::ensure!(
        after == keys,
        "snapshot refresh changed internal channel identity"
    );
    fixture.case.admin_proof(false).await?;
    let origin = fixture.source.origin();
    let playlist =
        format!("{origin}/playlist?credential=catalog-source-private&rotation=second-private");
    let guide = format!("{origin}/guide?credential=catalog-source-private&rotation=second-private");
    fixture
        .case
        .secrets
        .extend([playlist.clone(), guide.clone()]);
    assignment::patch(
        &fixture.case,
        json!({"operation":"rotate_credentials","playlist_url":playlist,"guide_url":guide}),
    )
    .await?
    .expect(StatusCode::OK)?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let retained = imports::status(
        &fixture.case.pool,
        &fixture.case.state.encryption_key,
        fixture.viewer.user,
    )
    .await?
    .ok_or_else(|| anyhow::anyhow!("retained status missing"))?;
    anyhow::ensure!(
        retained.snapshot_id == Some(second) && !retained.current_source,
        "credential rotation revived old snapshot data"
    );
    fixture.snapshot = fixture::refresh(&fixture.case).await??;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    assignment::patch(
        &fixture.case,
        json!({"operation":"revoke_destination","feed":"playlist"}),
    )
    .await?
    .expect(StatusCode::OK)?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    assignment::patch(&fixture.case,json!({"operation":"approve_destination","feed":"playlist","expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(StatusCode::OK)?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    fixture.snapshot = fixture::refresh(&fixture.case).await??;
    fixture.case.configure(false, None, true, true).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    fixture.case.configure(true, None, true, true).await?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::SERVICE_UNAVAILABLE)?;
    fixture.snapshot = fixture::refresh(&fixture.case).await??;
    fixture
        .source
        .set(b"#EXTM3U\n".to_vec(), b"<tv/>".to_vec())
        .await?;
    fixture.snapshot = fixture::refresh(&fixture.case).await??;
    let empty = api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    anyhow::ensure!(
        empty.value["items"] == json!([])
            && empty.value["has_more"] == false
            && empty.value.get("cursor").is_none()
            && empty.value["context"]["channel_count"] == 0,
        "valid empty lineup did not publish an empty page"
    );
    fixture
        .source
        .set(fixture::playlist(), fixture::guide())
        .await?;
    fixture.snapshot = fixture::refresh(&fixture.case).await??;
    let all: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM live_tv_catalog_snapshots WHERE household_user_id=$1",
    )
    .bind(fixture.viewer.user)
    .fetch_one(&fixture.case.pool)
    .await?;
    anyhow::ensure!(
        all == 1,
        "catalog refresh retained an unbounded snapshot archive"
    );
    println!("IPTV_CATALOG_STAGE last_good_generation_and_empty_passed");
    Ok(())
}
