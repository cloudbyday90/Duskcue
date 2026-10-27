// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use super::{PlaybackError, range::RangeSpec};
use crate::services::vod_lifecycle::{Content, Prepared, VodLifecycle};
use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use std::sync::Arc;

#[cfg(test)]
const CHUNK_BYTES: usize = crate::services::vod_lifecycle::CHUNK;

pub(super) async fn response_owned(
    owner: &Arc<VodLifecycle>,
    path: &Path,
    headers: &HeaderMap,
) -> Result<Response, PlaybackError> {
    response_scoped(owner, path, headers, None, None).await
}

pub(super) async fn response_scoped(
    owner: &Arc<VodLifecycle>,
    path: &Path,
    headers: &HeaderMap,
    viewer: Option<Arc<crate::services::vod_lifecycle::Generation>>,
    expected: Option<super::direct::FileIdentity>,
) -> Result<Response, PlaybackError> {
    let path = path.to_path_buf();
    let range_count = headers.get_all(header::RANGE).iter().count();
    let range_value = headers.get(header::RANGE).cloned();
    owner
        .response_scoped(None, viewer, move |ctx| {
            Box::pin(async move {
                let id = if expected.is_some() {
                    ctx.open_source(&path).await?
                } else {
                    ctx.open(&path, None).await?
                };
                let metadata = ctx.metadata(id).await?;
                if expected
                    .as_ref()
                    .is_some_and(|expected| !expected.matches_metadata(&metadata))
                {
                    return Err(PlaybackError::FileNotFound);
                }
                let size = metadata.len();
                let range = if range_count > 1 {
                    Err(PlaybackError::InvalidByteRange(
                        "multiple range fields".into(),
                    ))
                } else if let Some(value) = range_value.as_ref() {
                    value
                        .to_str()
                        .map_err(|_| PlaybackError::InvalidByteRange("non-ASCII range".into()))
                        .and_then(|value| RangeSpec::parse(Some(value), size))
                } else {
                    Ok(None)
                };
                let range = match range {
                    Ok(range) => range,
                    Err(_) => {
                        let response = Response::builder()
                            .status(StatusCode::RANGE_NOT_SATISFIABLE)
                            .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                            .header(header::CONTENT_LENGTH, "0")
                            .header(header::ACCEPT_RANGES, "bytes")
                            .header(header::CACHE_CONTROL, "private, no-store")
                            .body(Body::empty())
                            .expect("static range response headers");
                        let (parts, _) = response.into_parts();
                        return Ok(Prepared {
                            status: parts.status,
                            headers: parts.headers,
                            content: Content::Empty,
                        });
                    }
                };
                let (status, length) = if let Some(range) = range.as_ref() {
                    (StatusCode::PARTIAL_CONTENT, range.content_length())
                } else {
                    (StatusCode::OK, size)
                };
                let mut response = Response::builder()
                    .status(status)
                    .header(
                        header::CONTENT_TYPE,
                        super::service::guess_content_type(&path),
                    )
                    .header(header::CONTENT_LENGTH, length.to_string())
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CACHE_CONTROL, "private, no-store");
                let offset = range.as_ref().map_or(0, |range| range.start);
                if let Some(range) = range {
                    response = response.header(header::CONTENT_RANGE, range.content_range_header());
                }
                let response = response
                    .body(Body::empty())
                    .expect("static file response headers");
                let (parts, _) = response.into_parts();
                Ok(Prepared {
                    status: parts.status,
                    headers: parts.headers,
                    content: Content::File { id, offset, length },
                })
            })
        })
        .await
}

#[cfg(test)]
async fn response(path: &Path, headers: &HeaderMap) -> Result<Response, PlaybackError> {
    response_owned(&VodLifecycle::new(), path, headers).await
}

#[cfg(test)]
#[path = "file_delivery_tests.rs"]
mod tests;
