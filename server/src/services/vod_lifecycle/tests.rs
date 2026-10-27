// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::domains::playback::PlaybackError;
use axum::{body::to_bytes, http::StatusCode};
use std::{sync::Arc, time::Duration};
use tokio::sync::Notify;
use uuid::Uuid;

fn prepared(bytes: &'static [u8]) -> Prepared {
    Prepared {
        status: StatusCode::OK,
        headers: Default::default(),
        content: Content::Bytes(bytes.into()),
    }
}

#[tokio::test]
async fn viewer_groups_cancel_only_their_queues_without_using_generation_capacity() {
    let owner = VodLifecycle::new();
    let first = Generation::viewer();
    let second = Generation::viewer();
    let a = owner
        .response_scoped(None, Some(first.clone()), |_| {
            Box::pin(async {
                Ok(Prepared {
                    status: StatusCode::OK,
                    headers: Default::default(),
                    content: Content::Bytes(vec![1; CHUNK * 4].into()),
                })
            })
        })
        .await
        .unwrap();
    let b = owner
        .response_scoped(None, Some(second.clone()), |_| {
            Box::pin(async { Ok(prepared(b"other viewer")) })
        })
        .await
        .unwrap();
    let ids: Vec<_> = (0..128).map(|_| Uuid::now_v7()).collect();
    for id in &ids {
        owner.begin_generation(*id, false).unwrap();
    }
    tokio::time::timeout(Duration::from_secs(2), first.drain())
        .await
        .unwrap()
        .unwrap();
    assert!(to_bytes(a.into_body(), CHUNK * 4).await.is_err());
    assert_eq!(
        to_bytes(b.into_body(), 32).await.unwrap().as_ref(),
        b"other viewer"
    );
    assert!(second.active());
    second.drain().await.unwrap();
    owner.close();
    for id in ids {
        owner.finish_generation(id);
    }
    drain(&owner).await;
    assert!(owner.complete());
}

async fn drain(owner: &Arc<VodLifecycle>) {
    tokio::time::timeout(Duration::from_secs(3), async {
        owner.drain_commands().await.unwrap();
        owner.drain_responses().await.unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn unpolled_bodies_keep_capacity_until_root_discards_every_queue() {
    let owner = VodLifecycle::new();
    let mut responses = Vec::new();
    for _ in 0..128 {
        responses.push(
            owner
                .response(None, |_| Box::pin(async { Ok(prepared(b"owned")) }))
                .await
                .unwrap(),
        );
    }
    assert!(matches!(
        owner
            .response(None, |_| Box::pin(async { Ok(prepared(b"late")) }))
            .await,
        Err(PlaybackError::LifecycleUnavailable)
    ));
    owner.close();
    drain(&owner).await;
    assert!(owner.complete());
    for response in responses {
        assert!(to_bytes(response.into_body(), 32).await.is_err());
    }
}

#[tokio::test]
async fn generations_remain_counted_until_verified_retirement() {
    let owner = VodLifecycle::new();
    let ids: Vec<_> = (0..128).map(|_| Uuid::now_v7()).collect();
    for id in &ids {
        owner.begin_generation(*id, false).unwrap();
    }
    assert!(matches!(
        owner.begin_generation(Uuid::now_v7(), false),
        Err(PlaybackError::LifecycleUnavailable)
    ));
    let generation = owner.generation(ids[0]).unwrap();
    generation.retire();
    assert!(matches!(
        owner.begin_generation(Uuid::now_v7(), false),
        Err(PlaybackError::LifecycleUnavailable)
    ));
    generation.drain().await.unwrap();
    owner.finish_generation(ids[0]);
    let extra = Uuid::now_v7();
    owner.begin_generation(extra, false).unwrap();
    owner.close();
    for id in ids.into_iter().chain([extra]) {
        owner.finish_generation(id);
    }
    drain(&owner).await;
    assert!(owner.complete());
}

#[tokio::test]
async fn disconnected_command_retains_cleanup_and_root_waits_for_completion() {
    let owner = VodLifecycle::new();
    let started = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let path = std::env::temp_dir().join(format!("duskcue-retained-command-{}", Uuid::now_v7()));
    let task = {
        let owner = owner.clone();
        let started = started.clone();
        let release = release.clone();
        let path = path.clone();
        tokio::spawn(async move {
            owner
                .command(false, move |stop| async move {
                    tokio::fs::write(&path, b"owned")
                        .await
                        .map_err(|_| PlaybackError::LifecycleCleanup)?;
                    started.notify_one();
                    stop.cancelled().await;
                    release.notified().await;
                    tokio::fs::remove_file(&path)
                        .await
                        .map_err(|_| PlaybackError::LifecycleCleanup)?;
                    Ok(())
                })
                .await
        })
    };
    started.notified().await;
    task.abort();
    let _ = task.await;
    owner.close();
    assert!(path.exists());
    assert!(
        tokio::time::timeout(Duration::from_millis(30), owner.drain_commands())
            .await
            .is_err()
    );
    release.notify_one();
    drain(&owner).await;
    assert!(!path.exists());
    assert!(owner.complete());
}

#[tokio::test]
async fn cancellation_waits_for_owned_preparation_before_generation_cleanup() {
    let owner = VodLifecycle::new();
    let id = Uuid::now_v7();
    let generation = owner.begin_generation(id, false).unwrap();
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let response = {
        let owner = owner.clone();
        let entered = entered.clone();
        let release = release.clone();
        tokio::spawn(async move {
            owner
                .response(Some(id), move |_| {
                    Box::pin(async move {
                        entered.notify_one();
                        release.notified().await;
                        Ok(prepared(b"cancelled"))
                    })
                })
                .await
        })
    };
    entered.notified().await;
    response.abort();
    let _ = response.await;
    assert!(
        tokio::time::timeout(Duration::from_millis(30), generation.drain())
            .await
            .is_err()
    );
    release.notify_one();
    tokio::time::timeout(Duration::from_secs(3), generation.drain())
        .await
        .unwrap()
        .unwrap();
    owner.finish_generation(id);
    owner.close();
    drain(&owner).await;
    assert!(owner.complete());
}

#[tokio::test]
async fn unexpected_reader_exit_quarantines_generation_and_root() {
    let owner = VodLifecycle::new();
    let id = Uuid::now_v7();
    let generation = owner.begin_generation(id, false).unwrap();
    let result = owner
        .response(Some(id), |_| {
            Box::pin(async { panic!("owned fixture failure") })
        })
        .await;
    assert!(matches!(result, Err(PlaybackError::LifecycleCleanup)));
    assert!(!generation.active());
    owner.close();
    assert!(generation.drain().await.is_err());
    assert!(owner.drain_responses().await.is_err());
    assert!(!owner.complete());
}

#[tokio::test]
async fn actual_file_output_is_cancelled_without_polling_or_releasing_generation_early() {
    let owner = VodLifecycle::new();
    let id = Uuid::now_v7();
    let generation = owner.begin_generation(id, false).unwrap();
    let directory = std::env::temp_dir().join(format!("duskcue-owned-output-{id}"));
    tokio::fs::create_dir(&directory).await.unwrap();
    let path = directory.join("segment.m4s");
    tokio::fs::write(&path, vec![7; CHUNK * 8]).await.unwrap();
    let response = owner
        .response(Some(id), move |ctx| {
            Box::pin(async move {
                let file = ctx.open(&path, None).await?;
                Ok(Prepared {
                    status: StatusCode::OK,
                    headers: Default::default(),
                    content: Content::File {
                        id: file,
                        offset: 0,
                        length: (CHUNK * 8) as u64,
                    },
                })
            })
        })
        .await
        .unwrap();
    generation.drain().await.unwrap();
    remove_cache(&directory).await.unwrap();
    owner.finish_generation(id);
    owner.close();
    drain(&owner).await;
    assert!(owner.complete());
    assert!(to_bytes(response.into_body(), CHUNK * 8).await.is_err());
}

#[tokio::test]
async fn playlist_bounds_cover_input_and_regular_file_validation() {
    let owner = VodLifecycle::new();
    let directory =
        std::env::temp_dir().join(format!("duskcue-bounded-playlist-{}", Uuid::now_v7()));
    tokio::fs::create_dir(&directory).await.unwrap();
    let path = directory.join("manifest.m3u8");
    tokio::fs::write(&path, vec![b'x'; PLAYLIST + 1])
        .await
        .unwrap();
    let root = directory.clone();
    assert!(
        owner
            .response(None, move |ctx| Box::pin(async move {
                let _ = ctx.text(&path, &root).await?;
                Ok(prepared(b""))
            }))
            .await
            .is_err()
    );
    owner.close();
    drain(&owner).await;
    remove_cache(&directory).await.unwrap();
}

#[tokio::test]
async fn cancelled_http_response_remains_incomplete_over_real_tcp() {
    use axum::{Router, routing::get};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let owner = VodLifecycle::new();
    let route_owner = owner.clone();
    let path = std::env::temp_dir().join(format!("duskcue-http-output-{}", Uuid::now_v7()));
    let file = tokio::fs::File::create(&path).await.unwrap();
    file.set_len(100000000).await.unwrap();
    drop(file);
    let route_path = path.clone();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let stop = tokio_util::sync::CancellationToken::new();
    let server_stop = stop.clone();
    let app = Router::new().route(
        "/media",
        get(move || {
            let owner = route_owner.clone();
            let path = route_path.clone();
            async move {
                owner
                    .response(None, move |ctx| {
                        Box::pin(async move {
                            let id = ctx.open(&path, None).await?;
                            let mut prepared = prepared(b"");
                            prepared.content = Content::File {
                                id,
                                offset: 0,
                                length: 100000000,
                            };
                            prepared
                                .headers
                                .insert("content-length", "100000000".parse().unwrap());
                            Ok(prepared)
                        })
                    })
                    .await
                    .map_err(crate::error::AppError::from)
            }
        }),
    );
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(server_stop.cancelled_owned())
            .await
    });
    let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
    socket
        .write_all(b"GET /media HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let mut bytes = vec![0; 4096];
    let length = socket.read(&mut bytes).await.unwrap();
    assert!(length > 0);
    owner.close();
    drain(&owner).await;
    stop.cancel();
    let mut tail = Vec::new();
    let _ = socket.read_to_end(&mut tail).await;
    bytes.truncate(length);
    bytes.extend(tail);
    assert!(String::from_utf8_lossy(&bytes).starts_with("HTTP/1.1 200"));
    assert!(bytes.len() < 100000000);
    server.await.unwrap().unwrap();
    tokio::fs::remove_file(path).await.unwrap();
}
