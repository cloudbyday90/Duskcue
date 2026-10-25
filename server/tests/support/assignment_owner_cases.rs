// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::{self, Case};
use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

pub async fn run(case: &mut Case) -> anyhow::Result<()> {
    let viewing = http::Actor {
        user: case.owner.user,
        profile: case.owner.profile,
        token: http::session(&case.pool, case.owner.user, case.owner.profile).await?,
        cookie: false,
    };
    case.secrets.push(viewing.token.clone());
    case.owner_proof(true).await?;
    let owner_etag = case.owner_read().await?.etag()?;
    anyhow::ensure!(
        owner_etag != case.admin_read().await?.etag()?,
        "admin and owner validators were interchangeable"
    );
    let change =
        json!({"operation":"set_profile_access","profile_id":case.standard,"enabled":true});
    http::call(
        &case.app,
        Some(&viewing),
        "PATCH",
        http::OWNER,
        Some(change.clone()),
        &[("if-match", &owner_etag)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    let grant: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM household_owner_grants WHERE user_id=$1")
            .bind(case.owner.user)
            .fetch_one(&case.pool)
            .await?;
    case.owner_change(change.clone())
        .await?
        .expect(StatusCode::OK)?;
    let after: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT expires_at FROM household_owner_grants WHERE user_id=$1")
            .bind(case.owner.user)
            .fetch_one(&case.pool)
            .await?;
    anyhow::ensure!(grant == after, "policy write extended owner grant");
    http::call(
        &case.app,
        Some(&case.owner),
        "PATCH",
        http::OWNER,
        Some(change),
        &[("if-match", &owner_etag)],
    )
    .await?
    .expect(StatusCode::PRECONDITION_FAILED)?;
    case.owner_change(
        json!({"operation":"set_profile_access","profile_id":case.kids,"enabled":true}),
    )
    .await?
    .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    for profile in [case.other.profile, Uuid::now_v7()] {
        case.owner_change(
            json!({"operation":"set_profile_access","profile_id":profile,"enabled":true}),
        )
        .await?
        .expect(StatusCode::NOT_FOUND)?;
    }
    case.owner_change(json!({"operation":"set_live_limit","max_live_streams":4}))
        .await?
        .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    let narrowed = case
        .owner_change(json!({"operation":"set_live_limit","max_live_streams":2}))
        .await?
        .expect(StatusCode::OK)?;
    anyhow::ensure!(
        narrowed.value["configured_live_tv_cap"] == 2,
        "owner limit did not narrow admin allowance"
    );
    case.owner_change(json!({"operation":"set_live_limit","max_live_streams":null}))
        .await?
        .expect(StatusCode::OK)?;
    let missing = json!({"operation":"set_live_limit"});
    case.owner_change(missing)
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    for cap in [0, -3] {
        case.owner_change(json!({"operation":"set_live_limit","max_live_streams":cap}))
            .await?
            .expect(StatusCode::UNPROCESSABLE_ENTITY)?;
    }
    case.owner_change(json!({"operation":"set_live_limit","max_live_streams":2}))
        .await?
        .expect(StatusCode::OK)?;
    let page = http::call(
        &case.app,
        Some(&case.owner),
        "GET",
        &format!("{}/profiles?limit=1", http::OWNER),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        page.value["items"].as_array().is_some_and(|v| v.len() == 1)
            && page.value["next_cursor"].is_string(),
        "profile page was unbounded or lost cursor"
    );
    let cursor = page.value["next_cursor"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("profile cursor missing"))?;
    let next = http::call(
        &case.app,
        Some(&case.owner),
        "GET",
        &format!("{}/profiles?limit=100&after={cursor}", http::OWNER),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        next.value["items"]
            .as_array()
            .is_some_and(|v| v.iter().all(|v| v["id"] != page.value["items"][0]["id"])),
        "profile pagination repeated prior row"
    );
    let all = http::call(
        &case.app,
        Some(&case.owner),
        "GET",
        &format!("{}/profiles", http::OWNER),
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    anyhow::ensure!(
        all.value["items"].as_array().is_some_and(|v| v
            .iter()
            .any(|v| v["id"] == case.kids.to_string()
                && v["eligible"] == false
                && v["enabled"] == false)),
        "Kids profile was eligible"
    );
    for query in [
        "limit=0",
        "limit=101",
        "after=private-cursor",
        "unknown=true",
    ] {
        http::call(
            &case.app,
            Some(&case.owner),
            "GET",
            &format!("{}/profiles?{query}", http::OWNER),
            None,
            &[],
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    let old = case.owner_read().await?.etag()?;
    case.configure(true, Some(1), false, false).await?;
    http::call(
        &case.app,
        Some(&case.owner),
        "PATCH",
        http::OWNER,
        Some(json!({"operation":"set_live_limit","max_live_streams":null})),
        &[("if-match", &old)],
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    case.owner_proof(false).await?;
    let retained = case.owner_read().await?;
    anyhow::ensure!(
        retained.value["owner_max_live_streams"] == 2
            && retained.value["configured_live_tv_cap"] == 1,
        "delegation withdrawal broadened owner policy"
    );
    case.owner_change(json!({"operation":"set_live_limit","max_live_streams":null}))
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    case.owner_change(
        json!({"operation":"set_profile_access","profile_id":case.standard,"enabled":false}),
    )
    .await?
    .expect(StatusCode::FORBIDDEN)?;
    case.configure(true, Some(3), true, true).await?;
    case.owner_proof(false).await?;
    anyhow::ensure!(
        case.owner_read().await?.value["configured_live_tv_cap"] == 2,
        "admin broadening erased owner restriction"
    );
    sqlx::query("UPDATE household_owner_grants SET expires_at=clock_timestamp()-interval '1 second' WHERE user_id=$1").bind(case.owner.user).execute(&case.pool).await?;
    http::call(&case.app, Some(&case.owner), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    let stale = http::Actor {
        user: case.owner.user,
        profile: case.owner.profile,
        token: case.owner.token.clone(),
        cookie: case.owner.cookie,
    };
    case.owner_proof(false).await?;
    http::call(&case.app, Some(&stale), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::UNAUTHORIZED)?;
    let kids = http::Actor {
        user: case.owner.user,
        profile: case.kids,
        token: http::session(&case.pool, case.owner.user, case.kids).await?,
        cookie: false,
    };
    case.secrets.push(kids.token.clone());
    http::call(&case.app, Some(&kids), "GET", http::OWNER, None, &[])
        .await?
        .expect(StatusCode::FORBIDDEN)?;
    Ok(())
}
