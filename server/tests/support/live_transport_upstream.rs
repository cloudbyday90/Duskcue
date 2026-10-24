// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::{
    Router,
    body::{Body, Bytes},
    extract::{Path, State},
    http::{Response, StatusCode},
    routing::get,
};
use std::net::SocketAddr;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

#[derive(Clone)]
struct Fixture {
    bytes: Arc<Vec<u8>>,
    cancel: CancellationToken,
    tracker: TaskTracker,
    active: Arc<AtomicUsize>,
    traps: Arc<AtomicUsize>,
}

pub struct Upstream {
    pub address: SocketAddr,
    fixture: Fixture,
    server: tokio::task::JoinHandle<std::io::Result<()>>,
}

impl Upstream {
    pub async fn start(bytes: Vec<u8>) -> anyhow::Result<Self> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let fixture = Fixture {
            bytes: Arc::new(bytes),
            cancel: CancellationToken::new(),
            tracker: TaskTracker::new(),
            active: Arc::new(AtomicUsize::new(0)),
            traps: Arc::new(AtomicUsize::new(0)),
        };
        let app = Router::new()
            .route("/{mode}", get(reply))
            .with_state(fixture.clone());
        let cancelled = fixture.cancel.clone();
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(cancelled.cancelled_owned())
                .await
        });
        Ok(Self {
            address,
            fixture,
            server,
        })
    }
    pub fn active(&self) -> usize {
        self.fixture.active.load(Ordering::Acquire)
    }
    pub fn traps(&self) -> usize {
        self.fixture.traps.load(Ordering::Acquire)
    }
    pub async fn stop(self) -> anyhow::Result<()> {
        self.fixture.cancel.cancel();
        self.fixture.tracker.close();
        let active = self.fixture.active.clone();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            self.server.await??;
            self.fixture.tracker.wait().await;
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        anyhow::ensure!(
            active.load(Ordering::Acquire) == 0,
            "upstream connection still active"
        );
        Ok(())
    }
}

async fn reply(State(fixture): State<Fixture>, Path(mode): Path<String>) -> Response<Body> {
    if mode == "redirect" {
        return Response::builder()
            .status(302)
            .header("location", "/trap")
            .body(Body::empty())
            .unwrap();
    }
    if mode == "trap" {
        fixture.traps.fetch_add(1, Ordering::AcqRel);
        return Response::builder()
            .status(StatusCode::BAD_GATEWAY)
            .body(Body::empty())
            .unwrap();
    }
    if mode == "html" {
        return Response::builder()
            .header("content-type", "text/html")
            .body(Body::from("private-fixture-secret"))
            .unwrap();
    }
    if mode == "invalid" {
        return Response::builder()
            .header("content-type", "video/mp2t")
            .body(Body::from(vec![0u8; 600]))
            .unwrap();
    }
    let (sender, receiver) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(1);
    if mode == "eof" {
        return Response::builder()
            .header("content-type", "video/mp2t")
            .body(Body::empty())
            .unwrap();
    }
    fixture.active.fetch_add(1, Ordering::AcqRel);
    let task_fixture = fixture.clone();
    fixture.tracker.spawn(async move {
        for chunk in task_fixture.bytes.chunks(188 * 16) {
            if mode == "stall" {
                tokio::select! { _ = task_fixture.cancel.cancelled() => {}, _ = sender.closed() => {} }
                break;
            }
            tokio::select! { _ = task_fixture.cancel.cancelled() => break, result = sender.send(Ok(Bytes::copy_from_slice(chunk))) => if result.is_err() { break; } }
            tokio::select! { _ = task_fixture.cancel.cancelled() => break, _ = sender.closed() => break, _ = tokio::time::sleep(std::time::Duration::from_millis(60)) => {} }
        }
        task_fixture.active.fetch_sub(1, Ordering::AcqRel);
    });
    Response::builder()
        .header("content-type", "video/mp2t")
        .body(Body::from_stream(
            tokio_stream::wrappers::ReceiverStream::new(receiver),
        ))
        .unwrap()
}
