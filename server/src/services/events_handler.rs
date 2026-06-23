// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! SSE transport handler — exposes the [`EventBus`](super::event_bus::EventBus)
//! to HTTP clients at `GET /api/v1/events`.
//!
//! Implements the SSE wire transport spec'd in
//! [REAL_TIME_PUSH.md](../../../docs/design/REAL_TIME_PUSH.md):
//!
//! - Authentication via the standard session cookie (handled by
//!   [`AuthenticatedUser`](crate::extractors::AuthenticatedUser)).
//! - `?types=type1,type2` query filter — only events whose `event_type`
//!   matches are emitted on the connection. Absent = all events.
//! - `Last-Event-ID` request header — used to replay buffered events on
//!   reconnect. The bus keeps a per-user ring buffer of the last 100
//!   events.
//! - `X-Accel-Buffering: no` response header — documented nginx escape
//!   hatch for proxy buffering (see REAL_TIME_PUSH.md §Edge Cases).
//! - 15-second `KeepAlive` heartbeat — defeats idle timeouts (nginx 60s,
//!   Cloudflare 100s) and flushes proxy buffers when no events flow.
//!
//! ## Connection lifecycle
//!
//! The handler spawns a per-connection task that owns the broadcast
//! receiver, the replay buffer drain, the type-filter check, and —
//! critically — the [`ConnectionGuard`] (moved into the task). When the
//! client disconnects, the SSE response future is dropped, the
//! `ReceiverStream` sender closes, the spawned task observes the closed
//! channel on its next `tx.send().await` and exits, dropping the guard.
//! This guarantees the per-user connection count is accurate.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt as _;
use uuid::Uuid;

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::services::event_bus::{
    matches_filter, parse_type_filter, ConnectionLimitReached, ServerEvent,
};
use crate::state::AppState;

/// SSE keep-alive interval per REAL_TIME_PUSH.md §Heartbeat. Comfortably
/// below nginx's 60s `proxy_read_timeout` and Cloudflare's 100s idle
/// timeout.
const KEEPALIVE_INTERVAL_SECS: u64 = 15;

/// `retry:` field sent once on connection open — suggests a 5-second
/// reconnect delay to the browser's `EventSource` when the connection
/// drops. Per REAL_TIME_PUSH.md §Wire Format.
const RECONNECT_DELAY_MS: u64 = 5_000;

/// Channel depth between the per-connection forwarder task and the SSE
/// encoder. 32 is enough to absorb brief encoder stalls without lagging
/// the broadcast subscriber; on overflow the broadcast channel's lag
/// semantics apply upstream.
const FORWARDER_CHANNEL_DEPTH: usize = 32;

/// Query string parameters for `/api/v1/events`.
#[derive(Debug, Default, Deserialize)]
pub struct EventsQuery {
    /// Comma-separated event type filter
    /// (e.g., `transcode_progress,scan_progress`). Absent → all events.
    pub types: Option<String>,
}

/// Handler for `GET /api/v1/events`.
///
/// Returns an SSE response. The connection stays open until the client
/// disconnects or the server's graceful shutdown drops it.
pub async fn events_handler(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<EventsQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let user_id = user.user_id;
    let bus = state.event_bus.clone();

    let type_filter = parse_type_filter(query.types.as_deref());

    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| Uuid::parse_str(s).ok())
        .filter(|id| *id != Uuid::nil());

    // Enforce the per-user connection limit. The guard is moved into the
    // forwarder task below so the count decrements when the task exits
    // (which happens when the client disconnects).
    let conn_guard = match bus.register_connection(user_id) {
        Ok(guard) => guard,
        Err(ConnectionLimitReached { user_id, current, limit }) => {
            tracing::info!(
                user_id = %user_id,
                current,
                limit,
                "SSE connection rejected: per-user limit reached"
            );
            return Err(AppError::RateLimited {
                code: "SSE_LIMIT_REACHED".to_string(),
            });
        }
    };

    let replay_events = match last_event_id {
        Some(id) => bus.replay_after(user_id, id),
        None => Vec::new(),
    };
    let live_rx = bus.subscribe_stream(user_id);

    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(FORWARDER_CHANNEL_DEPTH);

    tokio::spawn(async move {
        let _conn_guard = conn_guard;

        if tx
            .send(Ok(Event::default().retry(Duration::from_millis(RECONNECT_DELAY_MS))))
            .await
            .is_err()
        {
            return;
        }

        for event in replay_events {
            if matches_filter(&event.event_type, type_filter.as_ref())
                && tx.send(Ok(encode_event(&event))).await.is_err()
            {
                return;
            }
        }

        let mut live_rx = live_rx;
        while let Some(result) = live_rx.next().await {
            match result {
                Ok(event) => {
                    if matches_filter(&event.event_type, type_filter.as_ref())
                        && tx.send(Ok(encode_event(&event))).await.is_err()
                    {
                        return;
                    }
                }
                Err(_lagged) => {
                    tracing::debug!(
                        "SSE subscriber lagged the broadcast channel; skipping"
                    );
                }
            }
        }
    });

    let stream = ReceiverStream::new(rx);
    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(KEEPALIVE_INTERVAL_SECS))
            .text("keep-alive"),
    );

    let mut extra_headers = HeaderMap::new();
    extra_headers.insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );

    Ok((extra_headers, sse).into_response())
}

/// Encode a [`ServerEvent`] as a complete SSE wire-format `Event` (with
/// `event:`, `id:`, and `data:` fields). The receiver emits this as a
/// single SSE frame followed by the trailing blank-line delimiter.
fn encode_event(event: &ServerEvent) -> Event {
    Event::default()
        .event(event.event_type.as_str())
        .data(event.payload.to_string())
        .id(event.id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `encode_event` constructs an `Event` via Axum's builder. Axum owns
    /// the wire-format encoder; we verify our payload serialization (the
    /// `data:` field contents) round-trips through `serde_json`.
    #[test]
    fn encode_event_payload_serializes_compactly() {
        let payload = json!({"library_id": "abc", "progress": 42});
        let compact = payload.to_string();
        assert_eq!(compact, r#"{"library_id":"abc","progress":42}"#);
    }

    #[test]
    fn encode_event_runs_without_panic() {
        let event = ServerEvent {
            id: Uuid::parse_str("01950abc-7def-4012-9b6c-4f8d2e1a0001").unwrap(),
            event_type: "storyboard_progress".to_string(),
            payload: json!({"progress": 42}),
        };
        let _encoded = encode_event(&event);
    }

    #[test]
    fn events_query_parses_types() {
        let q = EventsQuery {
            types: Some("a,b,c".to_string()),
        };
        assert_eq!(q.types.as_deref(), Some("a,b,c"));
    }

    #[test]
    fn events_query_default_is_none() {
        let q = EventsQuery::default();
        assert!(q.types.is_none());
    }
}
