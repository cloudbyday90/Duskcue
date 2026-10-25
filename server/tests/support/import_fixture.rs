// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{
    assignment,
    http::Case,
    peer::{self, Behavior, Peer, Security},
};
use axum::http::StatusCode;
use duskcue::domains::live_tv::feed::imports::{self, ImportOwner, ImportStatus};
use serde_json::json;
use std::{io::Write, net::IpAddr};
use uuid::Uuid;

pub fn playlist() -> Vec<u8> {
    format!("#EXTM3U\n#EXTINF:-1 tvg-id=\"shared-guide\" tvg-logo=\"http://127.0.0.1:9/nested-logo-private\",Channel-private\nhttps://stream.invalid/prefix/proxy/ts/stream/{}?token=stream-private\n#EXTINF:-1 tvg-id=\"shared-guide\",Other-private\nhttps://stream.invalid/proxy/ts/stream/{}?token=other-private\n",Uuid::from_u128(1),Uuid::from_u128(2)).into_bytes()
}
pub fn guide() -> Vec<u8> {
    b"<?xml version='1.0' encoding='UTF-8'?><tv><channel id='shared-guide'><display-name lang='en'>Guide-private</display-name></channel><channel id='shared-guide'><display-name lang='en'>Guide-private</display-name></channel><programme channel='shared-guide' start='20261025120000' stop='20261025130000 +0000'><title lang='en'>Programme-private</title><rating system='raw-private'><value>Unmapped</value></rating></programme><programme channel='shared-guide' start='2026'><title>Partial-private</title></programme><programme channel='unmatched' start='20261025120000'><title>Ignored-private</title></programme></tv>".to_vec()
}
pub fn gzip(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}
pub async fn peers(playlist: &[u8], guide: &[u8]) -> anyhow::Result<(Peer, Peer)> {
    let ip: IpAddr = "127.0.0.1".parse()?;
    Ok((
        Peer::new(
            ip,
            Security::Plain,
            Behavior::Reply(peer::response(playlist)),
        )
        .await?,
        Peer::new(ip, Security::Tls, Behavior::Reply(peer::response(guide))).await?,
    ))
}
pub async fn prepare(case: &mut Case, playlist: &Peer, guide: &Peer) -> anyhow::Result<()> {
    let a = playlist.origin("http", "duskcue-feed.invalid");
    let b = guide.origin("https", "duskcue-feed.invalid");
    let playlist_url = format!("{a}/private-feed?credential=feed-query-private");
    let guide_url = format!("{b}/private-feed?credential=feed-query-private");
    case.secrets
        .extend([playlist_url.clone(), guide_url.clone()]);
    assignment::patch(
        case,
        json!({"operation":"rotate_credentials","playlist_url":playlist_url,"guide_url":guide_url}),
    )
    .await?
    .expect(StatusCode::OK)?;
    for (feed, origin) in [("playlist", a), ("guide", b)] {
        assignment::patch(case,json!({"operation":"approve_destination","feed":feed,"expected_origin":origin,"addresses":["127.0.0.1"]})).await?.expect(StatusCode::OK)?;
    }
    Ok(())
}
pub fn owner(case: &Case) -> anyhow::Result<ImportOwner> {
    Ok(ImportOwner::qualification_fixture(
        case.pool.clone(),
        case.state.encryption_key.clone(),
    )?)
}
pub async fn status(case: &Case) -> anyhow::Result<ImportStatus> {
    imports::status(&case.pool, &case.state.encryption_key, case.owner.user)
        .await?
        .ok_or_else(|| anyhow::anyhow!("import status absent"))
}
pub async fn run(
    case: &mut Case,
    playlist: &[u8],
    guide: &[u8],
) -> anyhow::Result<Result<Uuid, imports::ImportError>> {
    let (a, b) = peers(playlist, guide).await?;
    prepare(case, &a, &b).await?;
    let owner = owner(case)?;
    let result = owner.start(case.owner.user, 0, 0)?.finish().await;
    anyhow::ensure!(owner.idle(), "import admission was released incorrectly");
    owner.shutdown().await?;
    a.finish(true).await?;
    b.finish(true).await?;
    Ok(result)
}
pub async fn count(case: &Case, table: &str) -> anyhow::Result<i64> {
    let sql = match table {
        "snapshots" => "SELECT count(*) FROM live_tv_catalog_snapshots",
        "channels" => "SELECT count(*) FROM live_tv_catalog_channels",
        "guides" => "SELECT count(*) FROM live_tv_catalog_guides",
        "programmes" => "SELECT count(*) FROM live_tv_catalog_programmes",
        _ => anyhow::bail!("unknown fixture table"),
    };
    Ok(sqlx::query_scalar(sql).fetch_one(&case.pool).await?)
}
