// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::*;
use crate::services::event_bus::{EventBus, ServerEvent};
use std::{
    collections::HashSet,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

async fn idle(owner: &EventStreams) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while !owner.idle() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn cancellation_ends_initial_retry_replay_and_live_capacity_waits() {
    for phase in ["retry", "replay", "live"] {
        let bus = Arc::new(EventBus::new(1));
        let user = Uuid::now_v7();
        let marker = ServerEvent::new("hint", serde_json::json!({}));
        let last = marker.id;
        bus.publish(user, marker);
        if phase == "replay" {
            for _ in 0..4 {
                bus.publish(user, ServerEvent::new("hint", serde_json::json!({})));
            }
        }
        let stop = CancellationToken::new();
        let (tx, mut rx) = mpsc::channel(1);
        if phase == "retry" {
            tx.try_send(Ok(wire::retry())).unwrap();
        }
        let mut run = std::pin::pin!(forwarding::run(
            bus.clone(),
            user,
            Some(last),
            None,
            tx,
            &stop,
            None,
        ));
        let mut cx = Context::from_waker(Waker::noop());
        assert!(run.as_mut().poll(&mut cx).is_pending());
        if phase == "replay" {
            let _queued = rx.try_recv().unwrap().unwrap();
            assert!(run.as_mut().poll(&mut cx).is_pending());
        }
        if phase == "live" {
            let _queued = rx.try_recv().unwrap().unwrap();
            for _ in 0..2 {
                bus.publish(user, ServerEvent::new("hint", serde_json::json!({})));
            }
            assert!(run.as_mut().poll(&mut cx).is_pending());
        }
        stop.cancel();
        assert!(run.as_mut().poll(&mut cx).is_ready());
    }
}

#[tokio::test]
async fn cancellation_discards_full_output_and_remains_eof_with_sender_alive() {
    let stop = CancellationToken::new();
    let (tx, rx) = mpsc::channel(wire::CHANNEL_DEPTH);
    for _ in 0..wire::CHANNEL_DEPTH {
        tx.try_send(Ok(wire::retry())).unwrap();
    }
    assert_eq!(tx.capacity(), 0);
    let mut out = Output::new(rx, stop.clone());
    stop.cancel();
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        assert!(matches!(
            Pin::new(&mut out).poll_next(&mut cx),
            Poll::Ready(None)
        ));
    }
    assert!(tx.is_closed());
    use axum::response::{
        IntoResponse,
        sse::{KeepAlive, Sse},
    };
    let body = Sse::new(out)
        .keep_alive(KeepAlive::new())
        .into_response()
        .into_body();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), axum::body::to_bytes(body, 1024))
            .await
            .unwrap()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn output_cancellation_wakes_an_idle_body_independently_of_producer_drop() {
    let stop = CancellationToken::new();
    let (tx, rx) = mpsc::channel(1);
    let mut out = Output::new(rx, stop.clone());
    let body = tokio::spawn(async move { out.next().await.is_none() });
    tokio::task::yield_now().await;
    stop.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), body)
            .await
            .unwrap()
            .unwrap()
    );
    assert!(tx.is_closed());
}

#[tokio::test]
async fn completed_handshake_has_a_live_subscription_before_first_body_poll() {
    let owner = Arc::new(EventStreams::new());
    let bus = Arc::new(EventBus::new(1));
    let user = Uuid::now_v7();
    let mut output = owner.open(bus.clone(), user, None, None).await.unwrap();
    assert!(bus.publish(user, ServerEvent::new("hint", serde_json::json!({}))));
    for _ in 0..2 {
        let _event = tokio::time::timeout(Duration::from_secs(1), output.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
    owner.close();
    owner.drain().await.unwrap();
    assert_eq!(bus.total_connection_count(), 0);
}

#[tokio::test]
async fn idle_disconnect_releases_slot_and_preserves_other_connections() {
    let bus = Arc::new(EventBus::new(1));
    let owner = Arc::new(EventStreams::new());
    let other = Uuid::now_v7();
    let mut other_out = owner.open(bus.clone(), other, None, None).await.unwrap();
    let _retry = other_out.next().await.unwrap().unwrap();
    for filter in [None, Some(HashSet::from(["hint".to_owned()]))] {
        let user = Uuid::now_v7();
        let mut output = owner
            .open(bus.clone(), user, filter.clone(), None)
            .await
            .unwrap();
        let _retry = output.next().await.unwrap().unwrap();
        assert_eq!(bus.connection_count(user), 1);
        assert!(matches!(
            owner.open(bus.clone(), user, None, None).await,
            Err(crate::error::AppError::RateLimited { .. })
        ));
        drop(output);
        tokio::time::timeout(Duration::from_secs(1), async {
            while bus.connection_count(user) != 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let replacement = owner.open(bus.clone(), user, filter, None).await.unwrap();
        assert_eq!(bus.connection_count(user), 1);
        drop(replacement);
    }
    assert_eq!(bus.connection_count(other), 1);
    bus.publish(
        other,
        ServerEvent::new("hint", serde_json::json!({"private":"other"})),
    );
    let _live = tokio::time::timeout(Duration::from_secs(1), other_out.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    owner.close();
    owner.drain().await.unwrap();
    assert_eq!(bus.total_connection_count(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_close_cannot_leave_late_registered_work_unjoined() {
    for round in 0..32 {
        let bus = Arc::new(EventBus::new(1));
        let owner = Arc::new(EventStreams::new());
        let barrier = Arc::new(tokio::sync::Barrier::new(2));
        let (o, b, g) = (owner.clone(), bus.clone(), barrier.clone());
        let admission = tokio::spawn(async move {
            g.wait().await;
            o.open(b, Uuid::now_v7(), None, None).await
        });
        barrier.wait().await;
        if round % 2 == 0 {
            tokio::time::timeout(Duration::from_secs(1), async {
                while bus.total_connection_count() == 0 && !admission.is_finished() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(bus.total_connection_count(), 1);
        }
        owner.close();
        let output = admission.await.unwrap();
        owner.drain().await.unwrap();
        drop(output);
        assert!(owner.idle());
        assert_eq!(bus.total_connection_count(), 0);
        assert!(matches!(
            owner.open(bus, Uuid::now_v7(), None, None).await,
            Err(crate::error::AppError::EventStream(_))
        ));
    }
}

#[tokio::test]
async fn unexpected_producer_exit_is_sticky_and_releases_guard() {
    let bus = Arc::new(EventBus::new(1));
    let owner = Arc::new(EventStreams::new());
    let user = Uuid::now_v7();
    let marker = ServerEvent::new("hint", serde_json::json!({}));
    let last = marker.id;
    bus.publish(user, marker);
    bus.publish(
        user,
        ServerEvent::new("qualification\ninvalid", serde_json::json!({})),
    );
    let _output = owner
        .open(bus.clone(), user, None, Some(last))
        .await
        .unwrap();
    idle(&owner).await;
    assert_eq!(bus.total_connection_count(), 0);
    assert!(matches!(
        owner.open(bus.clone(), user, None, None).await,
        Err(crate::error::AppError::EventStream(_))
    ));
    owner.close();
    assert!(owner.drain().await.is_err());
    assert!(owner.drain().await.is_err());
}

#[test]
fn encoded_payload_keeps_compact_json_and_uuidv7_identity() {
    let event = ServerEvent::new("hint", serde_json::json!({"progress":42}));
    assert_eq!(event.id.get_version_num(), 7);
    assert_eq!(event.payload.to_string(), "{\"progress\":42}");
    let _encoded = wire::encode(&event);
}
