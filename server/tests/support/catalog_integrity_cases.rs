// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    catalog_http as api,
    fixture::Fixture,
    routes::{self, Inventory},
};
use axum::http::StatusCode;
use uuid::Uuid;

async fn changed(
    fixture: &Fixture,
    id: Uuid,
    read: &'static str,
    write: &'static str,
    replacement: String,
    path: &str,
) -> anyhow::Result<()> {
    let original: String = sqlx::query_scalar(read)
        .bind(id)
        .fetch_one(&fixture.case.pool)
        .await?;
    sqlx::query(write)
        .bind(id)
        .bind(replacement)
        .execute(&fixture.case.pool)
        .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), path).await;
    sqlx::query(write)
        .bind(id)
        .bind(original)
        .execute(&fixture.case.pool)
        .await?;
    let reply = result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    anyhow::ensure!(
        reply.value["title"] == "IPTV_CATALOG_005"
            && reply.headers.get("retry-after").is_some_and(|v| v == "5"),
        "catalog integrity failure was not a safe retryable refusal"
    );
    Ok(())
}

pub async fn run(fixture: &Fixture, inventory: &Inventory) -> anyhow::Result<()> {
    let pool = &fixture.case.pool;
    let other: String =
        sqlx::query_scalar("SELECT metadata FROM live_tv_catalog_channels WHERE id=$1")
            .bind(inventory.other)
            .fetch_one(pool)
            .await?;
    changed(
        fixture,
        inventory.channel,
        "SELECT metadata FROM live_tv_catalog_channels WHERE id=$1",
        "UPDATE live_tv_catalog_channels SET metadata=$2 WHERE id=$1",
        other.clone(),
        api::ROOT,
    )
    .await?;
    changed(
        fixture,
        fixture.snapshot,
        "SELECT manifest FROM live_tv_catalog_snapshots WHERE id=$1",
        "UPDATE live_tv_catalog_snapshots SET manifest=$2 WHERE id=$1",
        other.clone(),
        api::ROOT,
    )
    .await?;
    let guide: Uuid =
        sqlx::query_scalar("SELECT guide_id FROM live_tv_catalog_channels WHERE id=$1")
            .bind(inventory.channel)
            .fetch_one(pool)
            .await?;
    changed(
        fixture,
        guide,
        "SELECT metadata FROM live_tv_catalog_guides WHERE id=$1",
        "UPDATE live_tv_catalog_guides SET metadata=$2 WHERE id=$1",
        other,
        api::ROOT,
    )
    .await?;
    let original: Vec<u8> =
        sqlx::query_scalar("SELECT channel_key FROM live_tv_catalog_channels WHERE id=$1")
            .bind(inventory.channel)
            .fetch_one(pool)
            .await?;
    let mut replacement = original.clone();
    replacement[0] ^= 128;
    sqlx::query("UPDATE live_tv_catalog_channels SET channel_key=$2 WHERE id=$1")
        .bind(inventory.channel)
        .bind(&replacement)
        .execute(pool)
        .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT).await;
    sqlx::query("UPDATE live_tv_catalog_channels SET channel_key=$2 WHERE id=$1")
        .bind(inventory.channel)
        .bind(original)
        .execute(pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let alternate: Uuid =
        sqlx::query_scalar("SELECT guide_id FROM live_tv_catalog_channels WHERE id=$1")
            .bind(inventory.undeclared)
            .fetch_one(pool)
            .await?;
    sqlx::query("UPDATE live_tv_catalog_channels SET guide_id=$2 WHERE id=$1")
        .bind(inventory.channel)
        .bind(alternate)
        .execute(pool)
        .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT).await;
    sqlx::query("UPDATE live_tv_catalog_channels SET guide_id=$2 WHERE id=$1")
        .bind(inventory.channel)
        .bind(guide)
        .execute(pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let programme = inventory.programmes[0];
    let path = routes::programme_path(fixture.snapshot, inventory.channel, false, 20, None);
    let flags: i32 =
        sqlx::query_scalar("SELECT quality_flags FROM live_tv_catalog_programmes WHERE id=$1")
            .bind(programme)
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "UPDATE live_tv_catalog_programmes SET quality_flags=MOD(quality_flags+1,16) WHERE id=$1",
    )
    .bind(programme)
    .execute(pool)
    .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), &path).await;
    sqlx::query("UPDATE live_tv_catalog_programmes SET quality_flags=$2 WHERE id=$1")
        .bind(programme)
        .bind(flags)
        .execute(pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let start: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT starts_at FROM live_tv_catalog_programmes WHERE id=$1")
            .bind(programme)
            .fetch_one(pool)
            .await?;
    sqlx::query(
        "UPDATE live_tv_catalog_programmes SET starts_at=starts_at+interval '1 minute' WHERE id=$1",
    )
    .bind(programme)
    .execute(pool)
    .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), &path).await;
    sqlx::query("UPDATE live_tv_catalog_programmes SET starts_at=$2 WHERE id=$1")
        .bind(programme)
        .bind(start)
        .execute(pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    let assignment: Uuid =
        sqlx::query_scalar("SELECT id FROM live_tv_assignments WHERE household_user_id=$1")
            .bind(fixture.viewer.user)
            .fetch_one(pool)
            .await?;
    let guide_destination: String =
        sqlx::query_scalar("SELECT guide_destination FROM live_tv_assignments WHERE id=$1")
            .bind(assignment)
            .fetch_one(pool)
            .await?;
    changed(
        fixture,
        assignment,
        "SELECT playlist_destination FROM live_tv_assignments WHERE id=$1",
        "UPDATE live_tv_assignments SET playlist_destination=$2 WHERE id=$1",
        guide_destination,
        api::ROOT,
    )
    .await?;
    let guide_secret: String =
        sqlx::query_scalar("SELECT guide_secret FROM live_tv_assignments WHERE id=$1")
            .bind(assignment)
            .fetch_one(pool)
            .await?;
    changed(
        fixture,
        assignment,
        "SELECT playlist_secret FROM live_tv_assignments WHERE id=$1",
        "UPDATE live_tv_assignments SET playlist_secret=$2 WHERE id=$1",
        guide_secret,
        api::ROOT,
    )
    .await?;
    sqlx::query("UPDATE live_tv_assignments SET playlist_address_count=2 WHERE id=$1")
        .bind(assignment)
        .execute(pool)
        .await?;
    let result = api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT).await;
    sqlx::query("UPDATE live_tv_assignments SET playlist_address_count=1 WHERE id=$1")
        .bind(assignment)
        .execute(pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    api::call(&fixture.case.app, Some(&fixture.viewer), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    println!("IPTV_CATALOG_STAGE contextual_integrity_and_approval_passed");
    Ok(())
}
