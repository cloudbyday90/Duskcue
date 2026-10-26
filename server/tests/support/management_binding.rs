// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http;
use axum::{Router, http::StatusCode};
use duskcue::state::AppState;
use serde_json::json;
use uuid::Uuid;

pub async fn run(app: &Router, state: &AppState) -> anyhow::Result<()> {
    let mut actor = http::actor(&state.pool, "standard", true).await?;
    sqlx::query("UPDATE users SET role='owner' WHERE id=$1")
        .bind(actor.user)
        .execute(&state.pool)
        .await?;
    http::unlock(app, state, &mut actor, true, &mut Vec::new()).await?;
    let path = format!("/api/v1/live-tv/households/{}/assignment", actor.user);
    let reply = http::call(app, Some(&actor), "PUT", &path, Some(json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE,"enabled":true,"max_live_streams":null,"allow_owner_profile_access":true,"allow_owner_live_limit":true})), &[("if-none-match","*")]).await?.expect(StatusCode::CREATED)?;
    let assignment_tag = reply.etag()?;
    http::unlock(app, state, &mut actor, false, &mut Vec::new()).await?;
    let context = http::call(
        app,
        Some(&actor),
        "GET",
        "/api/v1/auth/session-context",
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?
    .value["context_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let other = Uuid::now_v7();
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type) VALUES($1,$2,'Round trip','standard')").bind(other).bind(actor.user).execute(&state.pool).await?;
    for selected in [other, actor.profile] {
        sqlx::query("UPDATE user_sessions SET active_profile_id=$1 WHERE token_hash=$2")
            .bind(selected)
            .bind(duskcue::domains::household_security::operator::fingerprint(
                actor.token.as_bytes(),
            ))
            .execute(&state.pool)
            .await?;
    }
    http::call(
        app,
        Some(&actor),
        "GET",
        &path,
        None,
        &[("x-session-context", &context)],
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    http::unlock(app, state, &mut actor, false, &mut Vec::new()).await?;
    let refresh = format!("/api/v1/live-tv/households/{}/refresh", actor.user);
    let status = format!("{refresh}/status");
    let control = "/api/v1/live-tv/manual-refresh-control";
    let owner = "/api/v1/household-management/live-tv";
    let profiles = format!("{owner}/profiles?limit=20");
    let new_context = http::call(
        app,
        Some(&actor),
        "GET",
        "/api/v1/auth/session-context",
        None,
        &[],
    )
    .await?
    .expect(StatusCode::OK)?
    .value["context_id"]
        .as_str()
        .unwrap()
        .to_owned();
    anyhow::ensure!(context != new_context, "profile round trip revived binding");
    let control_before = http::call(app, Some(&actor), "GET", control, None, &[])
        .await?
        .expect(StatusCode::OK)?;
    let control_tag = control_before.etag()?;
    for route in [&path, &refresh, &status, control, owner, &profiles] {
        let reply = http::call(
            app,
            Some(&actor),
            "GET",
            route,
            None,
            &[("x-session-context", &context)],
        )
        .await?
        .expect(StatusCode::CONFLICT)?;
        anyhow::ensure!(
            reply.value["title"] == "OWNER_007",
            "binding refusal was not specific"
        );
        http::call(
            app,
            Some(&actor),
            "GET",
            route,
            None,
            &[("x-session-context", &new_context)],
        )
        .await?
        .expect(StatusCode::OK)?;
        http::call(
            app,
            Some(&actor),
            "GET",
            route,
            None,
            &[("x-session-context", "invalid")],
        )
        .await?
        .expect(StatusCode::BAD_REQUEST)?;
    }
    for (method, route, body, condition) in [
        (
            "PATCH",
            path.as_str(),
            Some(
                json!({"operation":"configure","enabled":false,"max_live_streams":null,"allow_owner_profile_access":false,"allow_owner_live_limit":false}),
            ),
            assignment_tag.as_str(),
        ),
        ("DELETE", path.as_str(), None, assignment_tag.as_str()),
        (
            "PATCH",
            owner,
            Some(json!({"operation":"set_live_limit","max_live_streams":1})),
            assignment_tag.as_str(),
        ),
        (
            "PUT",
            control,
            Some(json!({"enabled":true})),
            control_tag.as_str(),
        ),
        (
            "POST",
            refresh.as_str(),
            Some(json!({"playlist_pin":0,"guide_pin":0})),
            assignment_tag.as_str(),
        ),
    ] {
        http::call(
            app,
            Some(&actor),
            method,
            route,
            body,
            &[("x-session-context", &context), ("if-match", condition)],
        )
        .await?
        .expect(StatusCode::CONFLICT)?;
    }
    let vacant = format!(
        "/api/v1/live-tv/households/{}/assignment",
        http::actor(&state.pool, "standard", false).await?.user
    );
    http::call(
        app,
        Some(&actor),
        "PUT",
        &vacant,
        Some(json!({"playlist_url":http::PLAYLIST,"guide_url":http::GUIDE})),
        &[("x-session-context", &context), ("if-none-match", "*")],
    )
    .await?
    .expect(StatusCode::CONFLICT)?;
    anyhow::ensure!(
        http::call(app, Some(&actor), "GET", &path, None, &[])
            .await?
            .etag()?
            == assignment_tag,
        "stale write changed assignment"
    );
    anyhow::ensure!(
        http::call(app, Some(&actor), "GET", control, None, &[])
            .await?
            .etag()?
            == control_tag,
        "stale write changed control"
    );
    println!(
        "MANAGEMENT_BINDING_RESULT stale_profile_round_trip=true families=6 writes=6 no_side_effects=true"
    );
    Ok(())
}
