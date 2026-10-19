// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::path::Path;

use axum::body::Body;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use super::{PlaybackError, range::RangeSpec};

const CHUNK_BYTES: usize = 64 * 1024;

fn stream_body(reader: impl AsyncRead + Send + Unpin + 'static, length: u64) -> Body {
    Body::from_stream(ReaderStream::with_capacity(
        reader.take(length),
        CHUNK_BYTES,
    ))
}

pub(super) async fn response(path: &Path, headers: &HeaderMap) -> Result<Response, PlaybackError> {
    let mut options = tokio::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NONBLOCK);
    let mut file = options
        .open(path)
        .await
        .map_err(|_| PlaybackError::FileNotFound)?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| PlaybackError::FileNotFound)?;
    if !metadata.is_file() {
        return Err(PlaybackError::FileNotFound);
    }
    let size = metadata.len();
    let range = if headers.get_all(header::RANGE).iter().count() > 1 {
        Err(PlaybackError::InvalidByteRange(
            "multiple range fields".into(),
        ))
    } else if let Some(value) = headers.get(header::RANGE) {
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
            return Ok(Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{size}"))
                .header(header::CONTENT_LENGTH, "0")
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CACHE_CONTROL, "private, no-store")
                .body(Body::empty())
                .expect("static range response headers"));
        }
    };
    let (status, length) = if let Some(range) = range.as_ref() {
        file.seek(std::io::SeekFrom::Start(range.start))
            .await
            .map_err(|_| PlaybackError::FileNotFound)?;
        (StatusCode::PARTIAL_CONTENT, range.content_length())
    } else {
        (StatusCode::OK, size)
    };
    let mut response = Response::builder()
        .status(status)
        .header(
            header::CONTENT_TYPE,
            super::service::guess_content_type(path),
        )
        .header(header::CONTENT_LENGTH, length.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "private, no-store");
    if let Some(range) = range {
        response = response.header(header::CONTENT_RANGE, range.content_range_header());
    }
    Ok(response
        .body(stream_body(file, length))
        .expect("static file response headers"))
}

#[cfg(test)]
#[path = "file_delivery_tests.rs"]
mod tests;
