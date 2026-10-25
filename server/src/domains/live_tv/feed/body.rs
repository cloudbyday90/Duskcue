// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::FeedError;
use axum::{
    body::Body,
    http::{Response, StatusCode, header},
};
use hyper::body::Incoming;
use std::time::Duration;
use tokio_stream::StreamExt;

pub(super) async fn read(response: Response<Incoming>, limit: usize) -> Result<Vec<u8>, FeedError> {
    let headers = response.headers();
    if response.status() != StatusCode::OK
        || headers
            .get_all(header::CONTENT_ENCODING)
            .iter()
            .any(|v| !v.as_bytes().eq_ignore_ascii_case(b"identity"))
        || (headers.contains_key(header::TRANSFER_ENCODING)
            && headers.contains_key(header::CONTENT_LENGTH))
        || headers
            .get_all(header::TRANSFER_ENCODING)
            .iter()
            .any(|v| !v.as_bytes().eq_ignore_ascii_case(b"chunked"))
        || headers.get_all(header::TRANSFER_ENCODING).iter().count() > 1
    {
        return Err(FeedError::Response);
    }
    if let Some(length) = headers.get(header::CONTENT_LENGTH) {
        let length = length
            .to_str()
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .ok_or(FeedError::Response)?;
        if length > limit as u64 {
            return Err(FeedError::Response);
        }
    }
    let mut stream = Body::new(response.into_body()).into_data_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(5), stream.next())
        .await
        .map_err(|_| FeedError::Deadline)?
    {
        let chunk = chunk.map_err(|_| FeedError::Response)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(FeedError::Response);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
