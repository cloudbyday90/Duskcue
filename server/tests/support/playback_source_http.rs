// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use reqwest::header::HeaderMap;
use serde_json::{Value, json};
use uuid::Uuid;

use super::super::policy::ORIGIN;

pub(super) fn uuid(value: &Value, key: &str) -> anyhow::Result<Uuid> {
    Ok(value[key]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("source verification identity missing: {key}"))?
        .parse()?)
}

pub(super) fn headers(token: &str, client: &str) -> anyhow::Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    let (name, value) = if client == "browser" {
        (reqwest::header::COOKIE, format!("session={token}"))
    } else {
        (reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
    };
    headers.insert(name, value.parse()?);
    Ok(headers)
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    path: &str,
    headers: &HeaderMap,
) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        (path.starts_with("/api/v1/transcode/") || path.starts_with("/api/v1/stream/"))
            && !path.chars().any(|value| matches!(value, '?' | '#' | '\\'))
    );
    let mut response = client
        .get(format!("{ORIGIN}{path}"))
        .headers(headers.clone())
        .send()
        .await?;
    anyhow::ensure!(response.status() == reqwest::StatusCode::OK);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes
                .len()
                .checked_add(chunk.len())
                .is_some_and(|size| size <= 8 * 1024 * 1024)
        );
        bytes.extend_from_slice(&chunk);
    }
    anyhow::ensure!(!bytes.is_empty());
    Ok(bytes)
}

pub(super) async fn negatives(
    client: &reqwest::Client,
    path: &str,
    peer: &str,
    kids: &str,
) -> anyhow::Result<Value> {
    let url = format!("{ORIGIN}{path}");
    let anonymous = client.get(&url).send().await?;
    let peer = client.get(&url).bearer_auth(peer).send().await?;
    let kids = client.get(&url).bearer_auth(kids).send().await?;
    anyhow::ensure!(
        anonymous.status().as_u16() == 401
            && peer.status().as_u16() == 404
            && kids.status().as_u16() == 404
    );
    Ok(json!({"unauthenticated_status":401,"peer_status":404,"kids_status":404}))
}

pub(super) async fn fragments(
    client: &reqwest::Client,
    generation: Uuid,
    rendition: &str,
    headers: &HeaderMap,
) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let playlist = String::from_utf8(
        fetch(
            client,
            &format!("/api/v1/transcode/{generation}/manifest.m3u8"),
            headers,
        )
        .await?,
    )?;
    let segments = super::super::super::media::segments(&playlist)?;
    let init = playlist
        .lines()
        .find_map(|line| {
            line.strip_prefix("#EXT-X-MAP:URI=\"")
                .and_then(|value| value.strip_suffix('"'))
        })
        .ok_or_else(|| anyhow::anyhow!("source HLS initialization missing"))?;
    let prefix = format!("/api/v1/transcode/{generation}/{rendition}/");
    anyhow::ensure!(
        init == format!("{prefix}init.mp4")
            && !segments.is_empty()
            && segments.iter().all(|path| path.starts_with(&prefix))
    );
    Ok((
        fetch(client, init, headers).await?,
        fetch(client, &segments[0], headers).await?,
    ))
}
