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

//! Server→Client real-time event bus for SSE delivery.
//!
//! Implements the per-user pub/sub channel described in
//! [REAL_TIME_PUSH.md](../../docs/design/REAL_TIME_PUSH.md):
//!
//! - One [`tokio::sync::broadcast`] channel per user, lazily created on first
//!   subscribe or publish.
//! - A bounded ring buffer of recent events per user, drained on reconnect
//!   via the `Last-Event-ID` header.
//! - Per-user connection counter enforcing a configurable concurrent-SSE
//!   limit (default 5) to prevent abuse.
//!
//! The bus is intentionally process-local: Duskcue ships single-instance per
//! [MULTI_INSTANCE.md](../../docs/design/MULTI_INSTANCE.md), and a future
//! multi-instance deployment would replace this with a Redis/PG LISTEN layer.
//! The public API is designed so callers do not change when the backing
//! transport is upgraded.
//!
//! ## Event taxonomy
//!
//! Event types are documented in REAL_TIME_PUSH.md §Event Taxonomy. The
//! [`EventType`] enum here is deliberately non-exhaustive at the type level
//! (domain modules construct events with their own string type names —
//! `storyboard_progress`, `transcode_progress`, etc.) so new event sources
//! can land without touching this module. The canonical list lives in the
//! design doc.

use std::collections::VecDeque;
use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

/// Capacity of each per-user broadcast channel. Sized to absorb brief
/// subscriber stalls (e.g., a slow client) without lagging. The 100-event
/// ring buffer below provides durable replay for reconnects; the channel
/// capacity only needs to cover the latency between `publish()` and the
/// SSE handler's `recv()` loop.
const CHANNEL_CAPACITY: usize = 256;

/// Maximum events retained per user for `Last-Event-ID` replay. Per
/// REAL_TIME_PUSH.md: ~5 minutes of activity. Events older than the
/// buffer are recoverable via REST (notifications are persisted;
/// progress events overwrite prior state).
pub const RING_BUFFER_CAPACITY: usize = 100;

/// Default per-user concurrent SSE connection limit. Per REAL_TIME_PUSH.md
/// §Connection Limits. Configurable via `AuthConfig` in a future task.
pub const DEFAULT_MAX_CONNECTIONS_PER_USER: u32 = 5;

/// A server→client event ready to be serialized onto an SSE stream.
///
/// `id` is a UUIDv7 (naturally time-ordered, sortable for ring-buffer
/// replay). `event_type` is the snake_case type name that the client
/// filters on via `?types=`. `payload` is the JSON-encoded domain data
/// delivered in the SSE `data:` field.
#[derive(Debug, Clone, Serialize)]
pub struct ServerEvent {
    pub id: Uuid,
    pub event_type: String,
    pub payload: serde_json::Value,
}

impl ServerEvent {
    /// Construct a new event with a fresh UUIDv7 id.
    pub fn new(event_type: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: Uuid::now_v7(),
            event_type: event_type.into(),
            payload,
        }
    }
}

/// Per-user state: the broadcast sender, the replay ring buffer, and the
/// live connection counter.
#[derive(Debug)]
struct UserChannel {
    sender: broadcast::Sender<ServerEvent>,
    ring: std::sync::Mutex<VecDeque<ServerEvent>>,
    /// Current number of open SSE connections for this user. Incremented
    /// by [`EventBus::register_connection`] and decremented by the
    /// returned guard's `Drop` impl. Enforces the per-user connection
    /// limit.
    connections: std::sync::atomic::AtomicU32,
}

impl UserChannel {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            ring: std::sync::Mutex::new(VecDeque::with_capacity(RING_BUFFER_CAPACITY)),
            connections: std::sync::atomic::AtomicU32::new(0),
        }
    }
}

/// Drops a registered SSE connection. Returned by
/// [`EventBus::register_connection`] so the connection count is
/// decremented deterministically when the SSE handler's future is
/// dropped (client disconnect, server shutdown, etc.).
#[derive(Debug)]
#[clippy::has_significant_drop]
pub struct ConnectionGuard {
    bus: Arc<EventBus>,
    user_id: Uuid,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(entry) = self.bus.channels.get(&self.user_id) {
            entry
                .connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Error returned by [`EventBus::register_connection`] when the user has
/// reached the per-user concurrent-connection limit.
#[derive(Debug, thiserror::Error)]
#[error("SSE connection limit reached for user {user_id} ({current}/{limit})")]
pub struct ConnectionLimitReached {
    pub user_id: Uuid,
    pub current: u32,
    pub limit: u32,
}

/// Process-local event bus. Stored in `AppState` as `Arc<EventBus>` and
/// shared across all handlers and workers.
///
/// The internal `DashMap<Uuid, UserChannel>` shards per user; the broadcast
/// senders and ring buffers are lazily created on first use and never
/// removed (a one-time active user never re-incurs the allocation). The
/// memory cost is bounded: `CHANNEL_CAPACITY * sizeof(ServerEvent)` per
/// user for the broadcast buffer + `RING_BUFFER_CAPACITY` ring entries.
#[derive(Clone, Debug)]
pub struct EventBus {
    channels: Arc<DashMap<Uuid, Arc<UserChannel>>>,
    max_connections_per_user: u32,
}

impl EventBus {
    /// Construct a new bus with the given per-user connection limit.
    pub fn new(max_connections_per_user: u32) -> Self {
        Self {
            channels: Arc::new(DashMap::new()),
            max_connections_per_user,
        }
    }

    /// Construct with the default connection limit
    /// ([`DEFAULT_MAX_CONNECTIONS_PER_USER`]).
    pub fn with_default_limit() -> Self {
        Self::new(DEFAULT_MAX_CONNECTIONS_PER_USER)
    }

    /// Publish an event to a user's channel.
    ///
    /// The event is recorded in the per-user ring buffer (for `Last-Event-ID`
    /// replay) and broadcast to all live subscribers. If there are no current
    /// subscribers the event is still buffered — a client reconnecting within
    /// the ring-buffer window will receive it via replay.
    ///
    /// Returns `true` if the event was delivered to at least one live
    /// subscriber, `false` if there were no receivers (the event is still
    /// buffered for replay). Send errors (channel closed) are silently
    /// swallowed because channels are lazy and long-lived.
    pub fn publish(&self, user_id: Uuid, event: ServerEvent) -> bool {
        let channel = self.channel_for(user_id);

        {
            let mut ring = channel.ring.lock().expect("ring mutex poisoned");
            if ring.len() >= RING_BUFFER_CAPACITY {
                ring.pop_front();
            }
            ring.push_back(event.clone());
        }

        channel.sender.send(event).is_ok()
    }

    /// Subscribe to a user's channel, returning a fresh `Receiver` that will
    /// see all events published after this call. Does not register a
    /// connection; use [`EventBus::register_connection`] for that.
    pub fn subscribe(&self, user_id: Uuid) -> broadcast::Receiver<ServerEvent> {
        let channel = self.channel_for(user_id);
        channel.sender.subscribe()
    }

    /// Wrap a subscriber as a stream suitable for handing to Axum's `Sse`
    /// response. Convenience wrapper around `BroadcastStream::new`.
    pub fn subscribe_stream(&self, user_id: Uuid) -> BroadcastStream<ServerEvent> {
        BroadcastStream::new(self.subscribe(user_id))
    }

    /// Drain the per-user ring buffer for `Last-Event-ID` replay.
    ///
    /// Returns all events strictly newer than `last_event_id`, in publish
    /// order. UUIDv7 ids are time-ordered so a string comparison is safe,
    /// but we compare via `Uuid` for canonical correctness. If
    /// `last_event_id` is not found in the buffer (older than the buffer
    /// window), the entire buffer is returned — the client will receive
    /// redundant events for the buffer window, which is acceptable per
    /// REAL_TIME_PUSH.md (progress events are idempotent overwrites;
    /// notifications are de-duplicated client-side via the `id` field).
    pub fn replay_after(&self, user_id: Uuid, last_event_id: Uuid) -> Vec<ServerEvent> {
        let channel = self.channel_for(user_id);
        let ring = channel.ring.lock().expect("ring mutex poisoned");
        let mut seen = false;
        let mut out = Vec::new();
        for event in ring.iter() {
            if seen {
                out.push(event.clone());
            } else if event.id == last_event_id {
                seen = true;
            }
        }
        if !seen {
            ring.iter().cloned().collect()
        } else {
            out
        }
    }

    /// Register a new SSE connection against the per-user limit. Returns a
    /// guard that decrements the count on drop (when the SSE handler's
    /// future completes). Returns an error if the limit is exceeded.
    ///
    /// Connection registration is best-effort: a process crash may leave
    /// the count too high, but the limit is enforced on new connections
    /// and stale counts eventually drain as clients disconnect.
    pub fn register_connection(
        &self,
        user_id: Uuid,
    ) -> Result<ConnectionGuard, ConnectionLimitReached> {
        let channel = self.channel_for(user_id);
        let prev = channel
            .connections
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let current = prev + 1;
        if current > self.max_connections_per_user {
            channel
                .connections
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            return Err(ConnectionLimitReached {
                user_id,
                current: prev,
                limit: self.max_connections_per_user,
            });
        }
        Ok(ConnectionGuard {
            bus: Arc::new(EventBus {
                channels: self.channels.clone(),
                max_connections_per_user: self.max_connections_per_user,
            }),
            user_id,
        })
    }

    /// Current number of open SSE connections for a user. Best-effort
    /// (the count may briefly over-report around disconnects).
    pub fn connection_count(&self, user_id: Uuid) -> u32 {
        self.channels
            .get(&user_id)
            .map(|c| c.connections.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Total number of users with at least one subscribed channel. Used
    /// for the Prometheus `sse_connected_users` gauge.
    pub fn active_user_count(&self) -> usize {
        self.channels.len()
    }

    /// Get-or-create the channel for a user. Lazy creation means users who
    /// never connect incur zero allocation cost.
    fn channel_for(&self, user_id: Uuid) -> Arc<UserChannel> {
        if let Some(entry) = self.channels.get(&user_id) {
            return Arc::clone(&entry);
        }
        let new_channel = Arc::new(UserChannel::new());
        match self.channels.entry(user_id) {
            dashmap::mapref::entry::Entry::Occupied(entry) => Arc::clone(entry.get()),
            dashmap::mapref::entry::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&new_channel));
                new_channel
            }
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::with_default_limit()
    }
}

/// Parse the `?types=` filter query parameter (comma-separated event type
/// names) into a sorted set. Empty/absent → `None` (subscribe to all).
/// Whitespace around each type is trimmed.
pub fn parse_type_filter(types_param: Option<&str>) -> Option<std::collections::HashSet<String>> {
    let raw = types_param?.trim();
    if raw.is_empty() {
        return None;
    }
    let set: std::collections::HashSet<String> = raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if set.is_empty() { None } else { Some(set) }
}

/// Check whether an event passes the type filter. `None` = passthrough.
pub fn matches_filter(
    event_type: &str,
    filter: Option<&std::collections::HashSet<String>>,
) -> bool {
    match filter {
        None => true,
        Some(set) => set.contains(event_type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(t: &str, payload: &str) -> ServerEvent {
        ServerEvent::new(t, serde_json::json!({ "data": payload }))
    }

    #[test]
    fn publish_delivers_to_subscriber() {
        let bus = EventBus::with_default_limit();
        let mut rx = bus.subscribe(Uuid::nil());
        bus.publish(Uuid::nil(), ev("storyboard_progress", "42"));
        let received = rx.try_recv().expect("should have an event");
        assert_eq!(received.event_type, "storyboard_progress");
        assert_eq!(received.payload["data"], "42");
    }

    #[test]
    fn publish_buffers_for_replay_even_without_subscribers() {
        let bus = EventBus::with_default_limit();
        let event = ev("scan_progress", "1");
        bus.publish(Uuid::nil(), event.clone());
        let replayed = bus.replay_after(Uuid::nil(), Uuid::max());
        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].event_type, "scan_progress");
    }

    #[test]
    fn replay_returns_events_after_last_id() {
        let bus = EventBus::with_default_limit();
        let e1 = ev("a", "1");
        let e2 = ev("a", "2");
        let e3 = ev("a", "3");
        bus.publish(Uuid::nil(), e1.clone());
        bus.publish(Uuid::nil(), e2.clone());
        bus.publish(Uuid::nil(), e3.clone());

        let replayed = bus.replay_after(Uuid::nil(), e1.id);
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].id, e2.id);
        assert_eq!(replayed[1].id, e3.id);
    }

    #[test]
    fn replay_with_unknown_id_returns_entire_buffer() {
        let bus = EventBus::with_default_limit();
        bus.publish(Uuid::nil(), ev("a", "1"));
        bus.publish(Uuid::nil(), ev("a", "2"));
        let replayed = bus.replay_after(Uuid::nil(), Uuid::nil());
        assert_eq!(replayed.len(), 2);
    }

    #[test]
    fn ring_buffer_evicts_oldest_at_capacity() {
        let bus = EventBus::with_default_limit();
        for i in 0..(RING_BUFFER_CAPACITY + 10) {
            bus.publish(Uuid::nil(), ev("a", &i.to_string()));
        }
        let replayed = bus.replay_after(Uuid::nil(), Uuid::nil());
        assert_eq!(replayed.len(), RING_BUFFER_CAPACITY);
    }

    #[test]
    fn connection_limit_enforced() {
        let bus = EventBus::new(2);
        let user = Uuid::nil();
        let _g1 = bus.register_connection(user).expect("first allowed");
        let _g2 = bus.register_connection(user).expect("second allowed");
        let err = bus.register_connection(user).unwrap_err();
        assert_eq!(err.limit, 2);
        assert_eq!(err.current, 2);
        assert_eq!(bus.connection_count(user), 2);
    }

    #[test]
    fn connection_guard_decrements_on_drop() {
        let bus = EventBus::new(2);
        let user = Uuid::nil();
        {
            let _g = bus.register_connection(user).unwrap();
            assert_eq!(bus.connection_count(user), 1);
        }
        assert_eq!(bus.connection_count(user), 0);
    }

    #[test]
    fn connection_count_is_zero_for_unknown_user() {
        let bus = EventBus::with_default_limit();
        assert_eq!(bus.connection_count(Uuid::nil()), 0);
    }

    #[test]
    fn channel_reused_across_calls() {
        let bus = EventBus::with_default_limit();
        bus.publish(Uuid::nil(), ev("a", "1"));
        bus.publish(Uuid::nil(), ev("a", "2"));
        bus.publish(Uuid::nil(), ev("a", "3"));
        assert_eq!(bus.active_user_count(), 1);
    }

    #[test]
    fn lagged_subscribers_do_not_block_publish() {
        let bus = EventBus::with_default_limit();
        let mut slow_rx = bus.subscribe(Uuid::nil());
        for i in 0..(CHANNEL_CAPACITY + 10) {
            bus.publish(Uuid::nil(), ev("a", &i.to_string()));
        }
        let _ = slow_rx.try_recv();
    }

    #[test]
    fn parse_type_filter_handles_empty() {
        assert!(parse_type_filter(None).is_none());
        assert!(parse_type_filter(Some("")).is_none());
        assert!(parse_type_filter(Some("   ")).is_none());
        assert!(parse_type_filter(Some(",,")).is_none());
    }

    #[test]
    fn parse_type_filter_trims_and_collects() {
        let set = parse_type_filter(Some(" a , b ,c")).unwrap();
        assert_eq!(set.len(), 3);
        assert!(set.contains("a"));
        assert!(set.contains("b"));
        assert!(set.contains("c"));
    }

    #[test]
    fn matches_filter_passthrough_when_none() {
        assert!(matches_filter("anything", None));
    }

    #[test]
    fn matches_filter_respects_set() {
        let mut set = std::collections::HashSet::new();
        set.insert("a".to_string());
        assert!(matches_filter("a", Some(&set)));
        assert!(!matches_filter("b", Some(&set)));
    }

    #[test]
    fn event_id_is_unique_and_monotonic() {
        let e1 = ServerEvent::new("t", serde_json::Value::Null);
        let e2 = ServerEvent::new("t", serde_json::Value::Null);
        assert_ne!(e1.id, e2.id);
        assert!(e1.id < e2.id, "UUIDv7 should be time-ordered");
    }
}
