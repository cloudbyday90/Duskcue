// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use std::pin::Pin;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::task::{Context, Poll};

use axum::body::to_bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use tokio::io::{AsyncRead, AsyncSeekExt, AsyncWriteExt, ReadBuf};
use tokio_stream::StreamExt;

use super::{CHUNK_BYTES, response, stream_body};

struct Fixture(std::path::PathBuf);

impl Fixture {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!(
            "duskcue-file-delivery-{}.mp4",
            uuid::Uuid::now_v7()
        )))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[tokio::test]
async fn large_sparse_file_and_open_ended_range_stream_in_bounded_chunks() {
    let fixture = Fixture::new();
    let size = if cfg!(unix) {
        8 * 1024 * 1024 * 1024_u64 + 17
    } else {
        8 * 1024 * 1024 + 17
    };
    let mut file = tokio::fs::File::create(&fixture.0).await.unwrap();
    file.set_len(size).await.unwrap();
    file.seek(std::io::SeekFrom::Start(size - 4)).await.unwrap();
    file.write_all(b"tail").await.unwrap();
    file.flush().await.unwrap();
    drop(file);

    for range in [None, Some("bytes=0-")] {
        let mut headers = HeaderMap::new();
        if let Some(range) = range {
            headers.insert(header::RANGE, HeaderValue::from_static(range));
        }
        let result = response(&fixture.0, &headers).await.unwrap();
        assert_eq!(
            result.status(),
            if range.is_some() {
                StatusCode::PARTIAL_CONTENT
            } else {
                StatusCode::OK
            }
        );
        assert_eq!(result.headers()[header::CONTENT_LENGTH], size.to_string());
        let mut stream = result.into_body().into_data_stream();
        for _ in 0..3 {
            let chunk = stream.next().await.unwrap().unwrap();
            assert!(!chunk.is_empty());
            assert!(chunk.len() <= CHUNK_BYTES);
            assert!(chunk.iter().all(|byte| *byte == 0));
        }
        drop(stream);
    }

    let mut headers = HeaderMap::new();
    headers.insert(header::RANGE, HeaderValue::from_static("bytes=-4"));
    let result = response(&fixture.0, &headers).await.unwrap();
    assert_eq!(result.headers()[header::CONTENT_LENGTH], "4");
    assert_eq!(
        result.headers()[header::CONTENT_RANGE],
        format!("bytes {}-{}/{}", size - 4, size - 1, size)
    );
    assert_eq!(to_bytes(result.into_body(), 4).await.unwrap(), &b"tail"[..]);
}

#[tokio::test]
async fn empty_and_unsatisfiable_ranges_use_opened_file_size() {
    let fixture = Fixture::new();
    tokio::fs::write(&fixture.0, b"").await.unwrap();
    let result = response(&fixture.0, &HeaderMap::new()).await.unwrap();
    assert_eq!(result.status(), StatusCode::OK);
    assert_eq!(result.headers()[header::CONTENT_LENGTH], "0");
    assert!(to_bytes(result.into_body(), 0).await.unwrap().is_empty());

    for range in ["bytes=0-", "bytes=-1", "bytes=-0", "bytes=0-0,3-4"] {
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static(range));
        let result = response(&fixture.0, &headers).await.unwrap();
        assert_eq!(result.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(result.headers()[header::CONTENT_RANGE], "bytes */0");
        assert!(to_bytes(result.into_body(), 0).await.unwrap().is_empty());
    }

    tokio::fs::write(&fixture.0, b"12345").await.unwrap();
    let mut headers = HeaderMap::new();
    headers.append(header::RANGE, HeaderValue::from_static("bytes=0-1"));
    headers.append(header::RANGE, HeaderValue::from_static("bytes=3-4"));
    let result = response(&fixture.0, &headers).await.unwrap();
    assert_eq!(result.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(result.headers()[header::CONTENT_RANGE], "bytes */5");
}

struct TrackedReader {
    dropped: Arc<AtomicBool>,
}

impl AsyncRead for TrackedReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let length = buf.remaining().min(CHUNK_BYTES);
        static ZEROES: [u8; CHUNK_BYTES] = [0; CHUNK_BYTES];
        buf.put_slice(&ZEROES[..length]);
        Poll::Ready(Ok(()))
    }
}

impl Drop for TrackedReader {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn response_cancellation_releases_the_owned_reader() {
    let dropped = Arc::new(AtomicBool::new(false));
    let mut stream = stream_body(
        TrackedReader {
            dropped: dropped.clone(),
        },
        u64::MAX,
    )
    .into_data_stream();
    assert_eq!(stream.next().await.unwrap().unwrap().len(), CHUNK_BYTES);
    assert!(!dropped.load(Ordering::SeqCst));
    drop(stream);
    assert!(dropped.load(Ordering::SeqCst));
}

#[tokio::test]
async fn opened_length_limits_a_growing_reader() {
    let dropped = Arc::new(AtomicBool::new(false));
    let result = to_bytes(
        stream_body(
            TrackedReader {
                dropped: dropped.clone(),
            },
            17,
        ),
        17,
    )
    .await
    .unwrap();
    assert_eq!(result.len(), 17);
    assert!(dropped.load(Ordering::SeqCst));
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn cancelled_file_body_releases_its_actual_descriptor() {
    let fixture = Fixture::new();
    tokio::fs::write(&fixture.0, [0_u8; 128 * 1024])
        .await
        .unwrap();
    let descriptors = || {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| std::fs::read_link(entry.path()).ok().as_ref() == Some(&fixture.0))
            .count()
    };
    assert_eq!(descriptors(), 0);
    let result = response(&fixture.0, &HeaderMap::new()).await.unwrap();
    let mut stream = result.into_body().into_data_stream();
    assert!(stream.next().await.unwrap().unwrap().len() <= CHUNK_BYTES);
    assert_eq!(descriptors(), 1);
    drop(stream);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while descriptors() != 0 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
}
