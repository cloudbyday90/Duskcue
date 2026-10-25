// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use reqwest::{Client, Response};
use std::{net::SocketAddr, time::Duration};

pub fn client() -> anyhow::Result<Client> {
    Ok(Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(2))
        .build()?)
}

pub async fn request(
    client: &Client,
    address: SocketAddr,
    token: Option<&str>,
    cookie: bool,
    query: &str,
    last: Option<String>,
) -> anyhow::Result<Response> {
    tokio::time::sleep(Duration::from_millis(650)).await;
    let mut request = client.get(format!("http://{address}/api/v1/events{query}"));
    if let Some(token) = token {
        request = if cookie {
            request.header("cookie", format!("session={token}"))
        } else {
            request.bearer_auth(token)
        };
    }
    if let Some(last) = last {
        request = request.header("last-event-id", last);
    }
    let response = tokio::time::timeout(Duration::from_secs(3), request.send()).await??;
    anyhow::ensure!(
        response
            .headers()
            .get("cache-control")
            .is_some_and(|v| v == "no-store"),
        "SSE result was cacheable"
    );
    Ok(response)
}

pub async fn error(response: Response, status: u16) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        response.status().as_u16() == status,
        "SSE refusal status mismatch"
    );
    anyhow::ensure!(
        response
            .headers()
            .get("content-type")
            .is_some_and(|v| v == "application/problem+json"),
        "SSE refusal was not a problem response"
    );
    let mut peer = Peer {
        response,
        bytes: Vec::new(),
    };
    peer.end().await?;
    Ok(serde_json::from_slice(&peer.bytes)?)
}

pub struct Peer {
    response: Response,
    pub bytes: Vec<u8>,
}
impl Peer {
    pub fn new(response: Response) -> anyhow::Result<Self> {
        anyhow::ensure!(
            response.status() == 200
                && response
                    .headers()
                    .get("content-type")
                    .is_some_and(|v| v == "text/event-stream")
                && response
                    .headers()
                    .get("x-accel-buffering")
                    .is_some_and(|v| v == "no"),
            "SSE handshake/wire headers changed"
        );
        Ok(Self {
            response,
            bytes: Vec::new(),
        })
    }
    async fn chunk(&mut self) -> anyhow::Result<bool> {
        let chunk = tokio::time::timeout(Duration::from_secs(3), self.response.chunk()).await??;
        if let Some(chunk) = chunk {
            anyhow::ensure!(
                self.bytes.len() + chunk.len() <= 65536,
                "SSE fixture body exceeded bound"
            );
            self.bytes.extend_from_slice(&chunk);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub async fn until(&mut self, needle: &str) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(4), async {
            while !String::from_utf8_lossy(&self.bytes).contains(needle) {
                anyhow::ensure!(self.chunk().await?, "SSE ended before expected frame");
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(())
    }
    pub async fn end(&mut self) -> anyhow::Result<()> {
        tokio::time::timeout(Duration::from_secs(4), async {
            while self.chunk().await? {}
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        Ok(())
    }
}
