// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    catalog_http as api,
    fixture::Fixture,
    http,
    routes::{self, Inventory},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::feed::imports::ImportOwner;
use serde_json::json;
use uuid::Uuid;

pub async fn run(fixture: &mut Fixture, inventory: &Inventory) -> anyhow::Result<()> {
    fixture.case.admin_proof(false).await?;
    let case = &fixture.case;
    let path = format!("/api/v1/live-tv/households/{}/assignment", case.other.user);
    let origin = fixture.source.origin();
    http::call(&case.app,Some(&case.admin),"PUT",&path,Some(json!({"playlist_url":format!("{origin}/playlist?credential=catalog-source-private"),"guide_url":format!("{origin}/guide?credential=catalog-source-private"),"enabled":true,"allow_owner_profile_access":false,"allow_owner_live_limit":false})),&[("if-none-match","*")]).await?.expect(StatusCode::CREATED)?;
    for feed in ["playlist", "guide"] {
        let tag = http::call(&case.app, Some(&case.admin), "GET", &path, None, &[])
            .await?
            .etag()?;
        http::call(&case.app,Some(&case.admin),"PATCH",&path,Some(json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":["127.0.0.1"]})),&[("if-match",&tag)]).await?.expect(StatusCode::OK)?;
    }
    sqlx::query("INSERT INTO user_capabilities(user_id,capability,is_granted) VALUES ($1,'can_use_live_tv',true)").bind(case.other.user).execute(&case.pool).await?;
    sqlx::query("INSERT INTO live_tv_profile_access(household_user_id,assignment_id,profile_id,enabled) SELECT household_user_id,id,$2,true FROM live_tv_assignments WHERE household_user_id=$1").bind(case.other.user).bind(case.other.profile).execute(&case.pool).await?;
    let owner =
        ImportOwner::qualification_fixture(case.pool.clone(), case.state.encryption_key.clone())?;
    let imported = owner.start(case.other.user, 0, 0)?.finish().await;
    owner.shutdown().await?;
    let snapshot = imported?;
    sqlx::query("DELETE FROM household_owner_state WHERE user_id=$1")
        .bind(case.other.user)
        .execute(&case.pool)
        .await?;
    let requests = fixture.source.count();
    let reply = api::call(&case.app, Some(&case.other), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    anyhow::ensure!(
        reply.value["context"]["profile_id"] == case.other.profile.to_string()
            && reply.value["context"]["snapshot_id"] == snapshot.to_string(),
        "catalog crossed households"
    );
    api::call(
        &case.app,
        Some(&case.other),
        &routes::programme_path(fixture.snapshot, inventory.channel, false, 20, None),
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    api::call(
        &case.app,
        Some(&case.other),
        &routes::programme_path(snapshot, inventory.channel, false, 20, None),
    )
    .await?
    .expect(StatusCode::NOT_FOUND)?;
    api::call(
        &case.app,
        Some(&case.other),
        &format!("{}?limit=1&cursor={}", api::ROOT, inventory.cursor),
    )
    .await?
    .expect(StatusCode::BAD_REQUEST)?;
    let channel = Uuid::parse_str(
        reply.value["items"][0]["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("foreign household fixture entry missing"))?,
    )?;
    let original: String =
        sqlx::query_scalar("SELECT metadata FROM live_tv_catalog_channels WHERE id=$1")
            .bind(channel)
            .fetch_one(&case.pool)
            .await?;
    let foreign: String =
        sqlx::query_scalar("SELECT metadata FROM live_tv_catalog_channels WHERE id=$1")
            .bind(inventory.channel)
            .fetch_one(&case.pool)
            .await?;
    sqlx::query("UPDATE live_tv_catalog_channels SET metadata=$2 WHERE id=$1")
        .bind(channel)
        .bind(foreign)
        .execute(&case.pool)
        .await?;
    let result = api::call(&case.app, Some(&case.other), api::ROOT).await;
    sqlx::query("UPDATE live_tv_catalog_channels SET metadata=$2 WHERE id=$1")
        .bind(channel)
        .bind(original)
        .execute(&case.pool)
        .await?;
    result?.expect(StatusCode::SERVICE_UNAVAILABLE)?;
    api::call(&case.app, Some(&case.other), api::ROOT)
        .await?
        .expect(StatusCode::OK)?;
    let states: i64 =
        sqlx::query_scalar("SELECT count(*) FROM household_owner_state WHERE user_id=$1")
            .bind(case.other.user)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(
        states == 0 && fixture.source.count() == requests,
        "catalog initialized management state or fetched upstream data"
    );
    let tag = http::call(&case.app, Some(&case.admin), "GET", &path, None, &[])
        .await?
        .etag()?;
    http::call(
        &case.app,
        Some(&case.admin),
        "DELETE",
        &path,
        None,
        &[("if-match", &tag)],
    )
    .await?
    .expect(StatusCode::NO_CONTENT)?;
    api::call(&case.app, Some(&case.other), api::ROOT)
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let remaining: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM live_tv_catalog_snapshots WHERE household_user_id=$1",
    )
    .bind(case.other.user)
    .fetch_one(&case.pool)
    .await?;
    anyhow::ensure!(remaining == 0, "deleted assignment retained its catalog");
    println!("IPTV_CATALOG_STAGE household_isolation_and_read_only_state_passed");
    Ok(())
}
