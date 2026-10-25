// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::http::{self, PASSWORD, ROOT};
use axum::{Router, http::StatusCode};
use duskcue::domains::household_security::operator::issue_handoff;
use serde_json::json;
use sqlx::PgPool;

pub async fn run(app: &Router, pool: &PgPool, secrets: &mut Vec<String>) -> anyhow::Result<()> {
    secrets.push("928374651092".to_string());
    let mut actor = http::actor(pool, "standard", true).await?;
    secrets.push(actor.token.clone());
    let (method, _, headers) =
        http::call(app, Some(&actor), "GET", &format!("{ROOT}/password"), None).await?;
    anyhow::ensure!(
        method == StatusCode::METHOD_NOT_ALLOWED
            && headers
                .get("content-type")
                .is_none_or(|v| v != "application/problem+json"),
        "native method refusal was mislabeled as a problem body"
    );
    let viewer = http::Actor {
        user: actor.user,
        profile: actor.profile,
        token: http::session(pool, actor.user, actor.profile).await?,
        cookie: false,
    };
    secrets.push(viewer.token.clone());
    let handoff = issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    http::password(app, &mut actor, Some(&handoff), secrets).await?;
    let preferences = json!({"expected_profile_id":actor.profile,"viewing_preferences":{"autoplay_next_episode":false,"audio_language":"en","prefer_audio_description":false,"subtitle_mode":"none","subtitle_language":null,"prefer_sdh":false}});
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "PATCH",
            "/api/v1/profiles/current/viewing-preferences",
            Some(preferences)
        )
        .await?
        .0 == StatusCode::OK,
        "viewing preferences incorrectly required owner proof"
    );
    sqlx::query("UPDATE users SET role='owner' WHERE id=$1")
        .bind(actor.user)
        .execute(pool)
        .await?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            "/api/v1/profiles",
            Some(json!({"name":"Role bypass"}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "server-owner role bypassed activated proof"
    );
    sqlx::query("UPDATE users SET role='member' WHERE id=$1")
        .bind(actor.user)
        .execute(pool)
        .await?;
    let (code, child, _) = http::call(
        app,
        Some(&actor),
        "POST",
        "/api/v1/profiles",
        Some(
            json!({"name":"Exit PIN regression","profile_type":"kids","parent_pin":"928374651092"}),
        ),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::CREATED,
        "Kids regression profile creation failed"
    );
    let child = child["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Kids regression identity missing"))?;
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            &format!("/api/v1/profiles/{child}/switch"),
            Some(json!({}))
        )
        .await?
        .0 == StatusCode::OK,
        "protected household could not select Kids"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            &format!("{ROOT}/password"),
            Some(json!({"password":PASSWORD}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "Kids selection gained owner authority"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            &format!("/api/v1/profiles/{}/switch", actor.profile),
            Some(json!({}))
        )
        .await?
        .0 == StatusCode::FORBIDDEN,
        "Kids exit PIN was bypassed"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            "/api/v1/profiles/parent-unlock",
            Some(json!({"pin":"928374651092"}))
        )
        .await?
        .0 == StatusCode::OK,
        "Kids exit PIN regressed"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&viewer),
            "POST",
            &format!("/api/v1/profiles/{}/switch", actor.profile),
            Some(json!({}))
        )
        .await?
        .0 == StatusCode::OK,
        "parent-unlocked profile switch failed"
    );
    anyhow::ensure!(
        http::status(app, &viewer).await?["expires_at"].is_null(),
        "Kids exit PIN minted owner grant"
    );
    anyhow::ensure!(
        http::call(
            app,
            Some(&actor),
            "DELETE",
            &format!("/api/v1/profiles/{child}"),
            None
        )
        .await?
        .0 == StatusCode::OK,
        "Kids regression cleanup failed"
    );
    Ok(())
}
