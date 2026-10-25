// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use duskcue::domains::{
    auth::service::generate_session_token, household_security::operator::fingerprint,
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub const ROOT: &str = "/api/v1/household-management";
pub const PASSWORD: &str = "private-fixture-secret";

pub struct Actor {
    pub user: Uuid,
    pub profile: Uuid,
    pub token: String,
    pub cookie: bool,
}

pub async fn actor(pool: &PgPool, profile_type: &str, password: bool) -> anyhow::Result<Actor> {
    let user = Uuid::now_v7();
    let profile = Uuid::now_v7();
    let hash = if password {
        Some(duskcue::domains::auth::service::hash_password(PASSWORD)?)
    } else {
        None
    };
    sqlx::query("INSERT INTO users(id,username,display_name,role,password_hash) VALUES ($1,$2,'Owner security fixture','member',$3)")
        .bind(user).bind(format!("owner-fixture-{user}")).bind(hash).execute(pool).await?;
    sqlx::query("INSERT INTO user_profiles(id,owner_user_id,name,profile_type,is_default) VALUES ($1,$2,'Fixture',$3,true)")
        .bind(profile).bind(user).bind(profile_type).execute(pool).await?;
    let token = session(pool, user, profile).await?;
    Ok(Actor {
        user,
        profile,
        token,
        cookie: false,
    })
}

pub async fn session(pool: &PgPool, user: Uuid, profile: Uuid) -> anyhow::Result<String> {
    let token = generate_session_token();
    sqlx::query("INSERT INTO user_sessions(user_id,active_profile_id,token_hash,expires_at) VALUES ($1,$2,$3,clock_timestamp()+interval '1 hour')")
        .bind(user).bind(profile).bind(fingerprint(token.as_bytes())).execute(pool).await?;
    Ok(token)
}

pub async fn call(
    app: &Router,
    actor: Option<&Actor>,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> anyhow::Result<(StatusCode, Value, HeaderMap)> {
    call_with_challenge(app, actor, method, path, body, None).await
}

pub async fn call_with_challenge(
    app: &Router,
    actor: Option<&Actor>,
    method: &str,
    path: &str,
    body: Option<Value>,
    challenge: Option<&str>,
) -> anyhow::Result<(StatusCode, Value, HeaderMap)> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(challenge) = challenge {
        request = request.header("x-challenge-id", challenge);
    }
    if let Some(actor) = actor {
        request = if actor.cookie {
            request.header("cookie", format!("session={}", actor.token))
        } else {
            request.header("authorization", format!("Bearer {}", actor.token))
        };
    }
    let response = app
        .clone()
        .oneshot(request.body(body.map_or_else(Body::empty, |v| Body::from(v.to_string())))?)
        .await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 131072).await?;
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    if path.starts_with(ROOT) {
        anyhow::ensure!(
            headers.get("cache-control").and_then(|v| v.to_str().ok()) == Some("no-store"),
            "owner response was cacheable: {method} {path} {status}"
        );
    }
    if !status.is_success() {
        anyhow::ensure!(
            !value.to_string().contains(PASSWORD),
            "request secret appeared in error response"
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    Ok((status, value, headers))
}

pub async fn status(app: &Router, actor: &Actor) -> anyhow::Result<Value> {
    let (code, value, _) = call(app, Some(actor), "GET", &format!("{ROOT}/status"), None).await?;
    anyhow::ensure!(code == StatusCode::OK, "management status failed");
    Ok(value)
}

pub fn rotated(
    actor: &mut Actor,
    value: &Value,
    headers: &HeaderMap,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let token = if actor.cookie {
        anyhow::ensure!(
            value.get("session_token").is_none(),
            "cookie proof returned a body token"
        );
        let cookie = headers
            .get("set-cookie")
            .ok_or_else(|| anyhow::anyhow!("rotated cookie missing"))?
            .to_str()?;
        anyhow::ensure!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
            "rotated cookie protections missing"
        );
        cookie
            .split(';')
            .next()
            .and_then(|v| v.strip_prefix("session="))
            .ok_or_else(|| anyhow::anyhow!("rotated cookie malformed"))?
            .to_string()
    } else {
        value["session_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("rotated bearer missing"))?
            .to_string()
    };
    anyhow::ensure!(token != actor.token, "session was not rotated");
    secrets.push(token.clone());
    actor.token = token;
    Ok(())
}

pub async fn password(
    app: &Router,
    actor: &mut Actor,
    handoff: Option<&str>,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let (code, value, headers) = call(
        app,
        Some(actor),
        "POST",
        &format!("{ROOT}/password"),
        Some(json!({"password":PASSWORD,"handoff":handoff})),
    )
    .await?;
    anyhow::ensure!(code == StatusCode::OK, "password proof failed: {code}");
    rotated(actor, &value, &headers, secrets)
}

pub async fn finish(
    app: &Router,
    actor: &mut Actor,
    challenge: &Value,
    credential: Value,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let (code, value, headers) = call(
        app,
        Some(actor),
        "POST",
        &format!("{ROOT}/passkey/finish"),
        Some(json!({"challenge_id":challenge["challenge_id"],"credential":credential})),
    )
    .await?;
    anyhow::ensure!(code == StatusCode::OK, "passkey proof failed: {code}");
    rotated(actor, &value, &headers, secrets)
}

pub async fn start(app: &Router, actor: &Actor, input: Value) -> anyhow::Result<Value> {
    let (code, value, _) = call(
        app,
        Some(actor),
        "POST",
        &format!("{ROOT}/passkey/start"),
        Some(input),
    )
    .await?;
    anyhow::ensure!(
        code == StatusCode::OK,
        "passkey ceremony start failed: {code}"
    );
    Ok(value)
}
