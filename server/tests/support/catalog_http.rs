// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::http::Actor;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{HeaderMap, Request, StatusCode},
};
use serde_json::Value;
use std::{net::SocketAddr, time::Duration};
use tower::ServiceExt;

pub const ROOT: &str = "/api/v1/live-tv/catalog/channels";
pub struct Reply {
    pub status: StatusCode,
    pub value: Value,
    pub headers: HeaderMap,
}

impl Reply {
    pub fn expect(self, status: StatusCode) -> anyhow::Result<Self> {
        anyhow::ensure!(
            self.status == status,
            "catalog status {} expected {}",
            self.status,
            status
        );
        Ok(self)
    }
}

pub fn request(actor: Option<&Actor>, path: &str) -> anyhow::Result<Request<Body>> {
    let mut request = Request::builder().method("GET").uri(path);
    if let Some(actor) = actor {
        request = if actor.cookie {
            request.header("cookie", format!("session={}", actor.token))
        } else {
            request.header("authorization", format!("Bearer {}", actor.token))
        };
    }
    Ok(request.body(Body::empty())?)
}

pub async fn call(app: &Router, actor: Option<&Actor>, path: &str) -> anyhow::Result<Reply> {
    let response = tokio::time::timeout(
        Duration::from_secs(7),
        app.clone().oneshot(request(actor, path)?),
    )
    .await??;
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
    let value: Value = serde_json::from_slice(&bytes)?;
    check(&headers, status, &value)?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    Ok(Reply {
        status,
        value,
        headers,
    })
}

fn check(headers: &HeaderMap, status: StatusCode, value: &Value) -> anyhow::Result<()> {
    anyhow::ensure!(
        headers
            .get("cache-control")
            .is_some_and(|v| v == "no-store"),
        "catalog response was cacheable"
    );
    anyhow::ensure!(
        headers.get("content-type").is_some_and(|v| v
            == if status.is_success() {
                "application/json"
            } else {
                "application/problem+json"
            }),
        "catalog media type changed"
    );
    let encoded = value.to_string();
    for secret in [
        "stream-private",
        "other-private",
        "nested-logo-private",
        "shared-guide",
        "catalog-source-private",
        "duskcue-secret:v1:",
        "upstream_uuid",
        "guide_key",
        "channel_key",
        "playlist_secret",
        "guide_secret",
    ] {
        anyhow::ensure!(
            !encoded.contains(secret),
            "catalog exposed a secret-bearing field/reference"
        );
    }
    Ok(())
}

pub async fn tcp(address: SocketAddr, actor: &Actor) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(7))
        .build()?;
    let mut request = client.get(format!("http://{address}{ROOT}"));
    request = if actor.cookie {
        request.header("cookie", format!("session={}", actor.token))
    } else {
        request.bearer_auth(&actor.token)
    };
    let reply = request.send().await?;
    let status = reply.status();
    let headers = reply.headers().clone();
    anyhow::ensure!(
        status == 200 && reply.content_length().is_none_or(|n| n <= 1024 * 1024),
        "catalog TCP handshake failed"
    );
    let bytes = reply.bytes().await?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "catalog TCP response exceeded bound"
    );
    let value = serde_json::from_slice(&bytes)?;
    check(&headers, status, &value)?;
    tokio::time::sleep(Duration::from_millis(650)).await;
    Ok(())
}
