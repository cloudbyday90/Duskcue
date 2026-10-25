// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::{FeedError, body, head_limit, source::Source};
use axum::{body::Body, http::Request};
use hyper_util::rt::TokioIo;
use rustls::pki_types::ServerName;
use sqlx::PgPool;
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
use tokio_rustls::TlsConnector;
use url::{Host, Position};

trait Io: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Io for T {}

async fn connect(source: &Source, tls: &TlsConnector) -> Result<Box<dyn Io>, FeedError> {
    let stream = TcpStream::connect(source.address)
        .await
        .map_err(|_| FeedError::Connection)?;
    let peer = stream.peer_addr().map_err(|_| FeedError::Connection)?;
    if peer.ip().to_canonical() != source.address.ip().to_canonical()
        || peer.port() != source.address.port()
    {
        return Err(FeedError::Connection);
    }
    if source.url.scheme() == "http" {
        return Ok(Box::new(stream));
    }
    let name = match source.url.host().ok_or(FeedError::Authority)? {
        Host::Domain(name) => {
            ServerName::try_from(name.to_owned()).map_err(|_| FeedError::Authority)?
        }
        Host::Ipv4(ip) => ServerName::IpAddress(ip.into()),
        Host::Ipv6(ip) => ServerName::IpAddress(ip.into()),
    };
    tls.connect(name, stream)
        .await
        .map(|stream| Box::new(stream) as Box<dyn Io>)
        .map_err(|_| FeedError::Connection)
}

#[cfg(feature = "playback-qualification")]
pub(super) async fn fetch(
    source: &Source,
    pool: &PgPool,
    tls: &TlsConnector,
) -> Result<Vec<u8>, FeedError> {
    fetch_bounded(source, pool, tls, 65536).await
}

pub(super) async fn fetch_bounded(
    source: &Source,
    pool: &PgPool,
    tls: &TlsConnector,
    limit: usize,
) -> Result<Vec<u8>, FeedError> {
    let stream = tokio::time::timeout(Duration::from_secs(5), connect(source, tls))
        .await
        .map_err(|_| FeedError::Deadline)??;
    source.fence.current(pool).await?;
    let (stream, head_limit) = head_limit::bounded(stream);
    let (mut sender, connection) = hyper::client::conn::http1::Builder::new()
        .max_headers(64)
        .max_buf_size(32768)
        .handshake::<_, Body>(TokioIo::new(stream))
        .await
        .map_err(|_| FeedError::Connection)?;
    let request = Request::builder()
        .method("GET")
        .uri(&source.url[Position::BeforePath..Position::AfterQuery])
        .header(
            "host",
            &source.url[Position::BeforeHost..Position::AfterPort],
        )
        .header("accept-encoding", "identity")
        .header("connection", "close")
        .body(Body::empty())
        .map_err(|_| FeedError::Authority)?;
    let receive = async {
        let response = tokio::time::timeout(Duration::from_secs(5), sender.send_request(request))
            .await
            .map_err(|_| FeedError::Deadline)?
            .map_err(|_| FeedError::Response)?;
        head_limit.release();
        body::read(response, limit).await
    };
    tokio::pin!(receive, connection);
    tokio::select! {
        biased;
        result = &mut receive => result,
        result = &mut connection => {
            result.map_err(|_| FeedError::Response)?;
            receive.await
        }
    }
}
