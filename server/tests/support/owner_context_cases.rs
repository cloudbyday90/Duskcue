// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::{self, Actor};
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

async fn bound(
    app: &Router,
    actor: &Actor,
    method: &str,
    path: &str,
    body: Option<Value>,
    context: &str,
) -> anyhow::Result<(StatusCode, Value)> {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("x-session-context", context)
        .header("authorization", format!("Bearer {}", actor.token))
        .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))?;
    let response = app.clone().oneshot(request).await?;
    anyhow::ensure!(
        response
            .headers()
            .get("cache-control")
            .is_some_and(|value| value == "no-store"),
        "context response cacheable"
    );
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 262144).await?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    Ok((status, value))
}

pub async fn run(app: &Router, pool: &PgPool, secrets: &mut Vec<String>) -> anyhow::Result<()> {
    let path = "/api/v1/auth/session-context";
    let mut actor = http::actor(pool, "standard", true).await?;
    secrets.push(actor.token.clone());
    let (code, context, headers) = http::call(app, Some(&actor), "GET", path, None).await?;
    anyhow::ensure!(
        code == StatusCode::OK
            && headers
                .get("cache-control")
                .is_some_and(|v| v == "no-store"),
        "context read failed"
    );
    anyhow::ensure!(
        context["user_id"] == actor.user.to_string()
            && context["profile_id"] == actor.profile.to_string()
            && context["credential_revision"].is_string()
            && context["profile_context_revision"].is_string()
            && context["transport"] == "bearer",
        "context attribution failed"
    );
    let binding = context["context_id"].as_str().unwrap();
    let failures: i32 =
        sqlx::query_scalar("SELECT failed_attempts FROM household_owner_state WHERE user_id=$1")
            .bind(actor.user)
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);
    for (value, expected) in [
        ("malformed".to_string(), StatusCode::BAD_REQUEST),
        (format!("sc1.{}", "A".repeat(43)), StatusCode::CONFLICT),
    ] {
        let (code, _) = bound(
            app,
            &actor,
            "POST",
            &format!("{}/password", http::ROOT),
            Some(json!({"password":http::PASSWORD})),
            &value,
        )
        .await?;
        anyhow::ensure!(code == expected, "invalid context binding accepted");
    }
    let after: i32 =
        sqlx::query_scalar("SELECT failed_attempts FROM household_owner_state WHERE user_id=$1")
            .bind(actor.user)
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);
    anyhow::ensure!(
        after == failures,
        "binding rejection consumed proof attempts"
    );
    let handoff =
        duskcue::domains::household_security::operator::issue_handoff(pool, actor.user).await?;
    secrets.push(handoff.clone());
    let (code, proof, headers) = http::call(
        app,
        Some(&actor),
        "POST",
        &format!("{}/password", http::ROOT),
        Some(json!({"password":http::PASSWORD,"handoff":handoff})),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::OK
            && proof["context"]["session_id"] == context["session_id"]
            && proof["context"]["context_id"] != context["context_id"],
        "proof context did not rotate"
    );
    http::rotated(&mut actor, &proof, &headers, secrets)?;
    anyhow::ensure!(
        bound(
            app,
            &actor,
            "GET",
            &format!("{}/status", http::ROOT),
            None,
            binding
        )
        .await?
        .0 == StatusCode::CONFLICT,
        "old binding survived rotation"
    );
    let (_, fresh, _) = http::call(app, Some(&actor), "GET", path, None).await?;
    let fresh_binding = fresh["context_id"].as_str().unwrap();
    let (_, wrong) = bound(
        app,
        &actor,
        "POST",
        &format!("{}/password", http::ROOT),
        Some(json!({"password":"incorrect"})),
        fresh_binding,
    )
    .await?;
    anyhow::ensure!(
        wrong["title"] == "OWNER_002"
            && http::call(app, Some(&actor), "GET", path, None).await?.0 == StatusCode::OK,
        "bad proof expired viewing"
    );
    let other: uuid::Uuid = sqlx::query_scalar("INSERT INTO user_profiles(owner_user_id,name,profile_type) VALUES($1,'Other context','standard') RETURNING id").bind(actor.user).fetch_one(pool).await?;
    for profile in [other, actor.profile] {
        anyhow::ensure!(
            http::call(
                app,
                Some(&actor),
                "POST",
                &format!("/api/v1/profiles/{profile}/switch"),
                Some(json!({"remember_on_device":false}))
            )
            .await?
            .0 == StatusCode::OK,
            "profile switch failed"
        );
    }
    let (_, returned, _) = http::call(app, Some(&actor), "GET", path, None).await?;
    anyhow::ensure!(
        returned["profile_id"] == context["profile_id"]
            && returned["context_id"] != fresh["context_id"],
        "profile round trip revived binding"
    );
    anyhow::ensure!(
        bound(
            app,
            &actor,
            "DELETE",
            &format!("{}/grant", http::ROOT),
            None,
            fresh_binding
        )
        .await?
        .0 == StatusCode::CONFLICT,
        "old lock bound to new context"
    );
    let kids = http::actor(pool, "kids", false).await?;
    secrets.push(kids.token.clone());
    anyhow::ensure!(
        http::call(app, Some(&kids), "GET", path, None).await?.0 == StatusCode::OK,
        "viewing context rejected Kids"
    );
    anyhow::ensure!(
        http::call(app, None, "GET", path, None)
            .await?
            .2
            .get("cache-control")
            .is_some_and(|v| v == "no-store"),
        "unauthenticated context response cacheable"
    );
    Ok(())
}
