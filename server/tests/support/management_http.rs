// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[path = "household_actor.rs"]
pub mod actors;
pub use actors::{Actor, PASSWORD, actor};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use duskcue::{domains::household_security::operator, state::AppState};
use serde_json::{Value, json};
use tower::ServiceExt;

pub const OWNER: &str = "/api/v1/household-management/live-tv";
pub const PLAYLIST: &str =
    "http://127.0.0.1:9/reverse/profile.m3u?password=private-playlist-credential";
pub const GUIDE: &str = "https://guide.invalid/reverse/xmltv.php?password=private-guide-credential";
pub const ROTATED_PLAYLIST: &str =
    "https://source.invalid/profile/m3u?secret=rotated-playlist-private";
pub const ROTATED_GUIDE: &str = "http://127.0.0.1:9/profile/epg?secret=rotated-guide-private";

pub struct Reply {
    pub status: StatusCode,
    pub value: Value,
    pub headers: HeaderMap,
}

impl Reply {
    pub fn expect(self, expected: StatusCode) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.status == expected,
            "assignment status {} expected {}",
            self.status,
            expected
        );
        Ok(self)
    }
    pub fn etag(&self) -> anyhow::Result<String> {
        Ok(self
            .headers
            .get("etag")
            .ok_or_else(|| anyhow::anyhow!("assignment ETag missing"))?
            .to_str()?
            .to_string())
    }
}

pub async fn raw(app: &Router, request: Request<Body>) -> anyhow::Result<Reply> {
    let path = request.uri().path().to_string();
    let response = app.clone().oneshot(request).await?;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 131072).await?;
    let value = if bytes.is_empty() {
        Value::Null
    } else if headers
        .get("content-type")
        .is_some_and(|v| v.as_bytes().starts_with(b"application/"))
    {
        serde_json::from_slice(&bytes)?
    } else {
        Value::String(String::from_utf8(bytes.to_vec())?)
    };
    if path.starts_with("/api/v1/live-tv/") || path.starts_with(OWNER) {
        anyhow::ensure!(
            headers
                .get("cache-control")
                .is_some_and(|v| v == "no-store"),
            "assignment response was cacheable"
        );
        let encoded = value.to_string();
        anyhow::ensure!(
            [
                PLAYLIST,
                GUIDE,
                ROTATED_PLAYLIST,
                ROTATED_GUIDE,
                PASSWORD,
                "duskcue-secret:v1:"
            ]
            .iter()
            .all(|secret| !encoded.contains(secret)),
            "assignment response disclosed secret material"
        );
    }
    tokio::time::sleep(std::time::Duration::from_millis(650)).await;
    Ok(Reply {
        status,
        value,
        headers,
    })
}

pub async fn call(
    app: &Router,
    actor: Option<&Actor>,
    method: &str,
    path: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> anyhow::Result<Reply> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if let Some(actor) = actor {
        request = if actor.cookie {
            request.header("cookie", format!("session={}", actor.token))
        } else {
            request.header("authorization", format!("Bearer {}", actor.token))
        };
    }
    for (name, value) in headers {
        request
            .headers_mut()
            .ok_or_else(|| anyhow::anyhow!("fixture request headers unavailable"))?
            .insert(
                axum::http::HeaderName::from_bytes(name.as_bytes())?,
                axum::http::HeaderValue::from_str(value)?,
            );
    }
    raw(
        app,
        request.body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))?,
    )
    .await
}

pub async fn unlock(
    app: &Router,
    state: &AppState,
    actor: &mut Actor,
    first: bool,
    secrets: &mut Vec<String>,
) -> anyhow::Result<()> {
    let handoff = if first {
        let token = operator::issue_handoff(&state.pool, actor.user).await?;
        secrets.push(token.clone());
        Some(token)
    } else {
        None
    };
    let reply = call(
        app,
        Some(actor),
        "POST",
        "/api/v1/household-management/password",
        Some(json!({"password":PASSWORD,"handoff":handoff})),
        &[],
    )
    .await?
    .expect(StatusCode::OK)?;
    let token = if actor.cookie {
        anyhow::ensure!(
            reply.value.get("session_token").is_none(),
            "cookie owner proof returned a body token"
        );
        let cookie = reply
            .headers
            .get("set-cookie")
            .ok_or_else(|| anyhow::anyhow!("owner proof cookie missing"))?
            .to_str()?;
        anyhow::ensure!(
            cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"),
            "owner proof cookie protections missing"
        );
        cookie
            .split(';')
            .next()
            .and_then(|v| v.strip_prefix("session="))
            .ok_or_else(|| anyhow::anyhow!("owner proof cookie malformed"))?
            .to_string()
    } else {
        reply.value["session_token"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("owner proof bearer missing"))?
            .to_string()
    };
    anyhow::ensure!(token != actor.token, "owner proof did not rotate session");
    secrets.push(token.clone());
    actor.token = token;
    Ok(())
}
