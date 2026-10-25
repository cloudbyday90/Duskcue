// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::wire;
use crate::services::event_bus::{EventBus, matches_filter};
use axum::response::sse::Event;
use std::{collections::HashSet, convert::Infallible, sync::Arc};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) async fn send(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    stop: &CancellationToken,
    event: Event,
) -> bool {
    tokio::select! {
        biased;
        _ = stop.cancelled() => false,
        _ = tx.closed() => false,
        result = tx.send(Ok(event)) => result.is_ok(),
    }
}

pub(super) async fn run(
    bus: Arc<EventBus>,
    user: Uuid,
    last: Option<Uuid>,
    filter: Option<HashSet<String>>,
    tx: mpsc::Sender<Result<Event, Infallible>>,
    stop: &CancellationToken,
    prepared: Option<oneshot::Sender<()>>,
) {
    if stop.is_cancelled() || tx.is_closed() {
        return;
    }
    let replay = last
        .map(|id| bus.replay_after(user, id))
        .unwrap_or_default();
    let mut live = bus.subscribe_stream(user);
    if let Some(prepared) = prepared {
        let _ = prepared.send(());
    }
    if !send(&tx, stop, wire::retry()).await {
        return;
    }
    for event in replay {
        if stop.is_cancelled() || tx.is_closed() {
            return;
        }
        if matches_filter(&event.event_type, filter.as_ref())
            && !send(&tx, stop, wire::encode(&event)).await
        {
            return;
        }
    }
    loop {
        let event = tokio::select! {
            biased;
            _ = stop.cancelled() => return,
            _ = tx.closed() => return,
            event = live.next() => match event { Some(event) => event, None => return },
        };
        match event {
            Ok(event) => {
                if stop.is_cancelled() || tx.is_closed() {
                    return;
                }
                if matches_filter(&event.event_type, filter.as_ref())
                    && !send(&tx, stop, wire::encode(&event)).await
                {
                    return;
                }
            }
            Err(_) => tracing::debug!(event = "sse_subscriber_lagged"),
        }
    }
}
