# Real-Time Push

## Overview

This document is the authoritative design for how the Duskcue server pushes events to connected clients (web browser, Smart TV, Tauri desktop, Flutter mobile) in real time. The goal is to deliver state changes — transcode progress, scan progress, notifications, admin session kicks, remote-control commands — without requiring clients to poll REST endpoints.

The decision documented here: **adopt Server-Sent Events (SSE) as Duskcue's sole real-time push mechanism**. WebSocket was previously spec'd in [API_CONVENTIONS.md](API_CONVENTIONS.md) but is superseded by this document. Long-polling and WebTransport are considered and rejected.

## Scope — What This Document Covers

**Covers:**

- Transport choice (SSE vs WebSocket vs long-polling vs WebTransport)
- Event taxonomy, wire format, endpoint structure
- Authentication and authorization of event streams
- Reconnection, missed-event recovery, heartbeat/keepalive
- Edge cases: proxies/CDNs, mobile OS backgrounding, multi-tab connection sharing
- Implementation status across the codebase

**Does NOT cover:**

- HTTP-layer caching of normal request/response traffic — see [HTTP_CACHING.md](HTTP_CACHING.md)
- WebSocket-based voice/video chat (not a Duskcue feature)
- Mobile OS push notifications (FCM/APNs) — Phase 16 concern; documented here only as a complement to SSE for offline delivery
- The actual event payload schemas for each event type — those live with their domain (transcode events in [STREAMING.md](STREAMING.md), scan events in [MEDIA_SCANNING.md](MEDIA_SCANNING.md), notifications in Phase 13)

## Decision — SSE Over WebSocket

**Duskcue adopts Server-Sent Events (SSE) — RFC 8895 / HTML5 `EventSource` — as the only real-time push transport.**

### Why SSE Is the Right Fit

Every Duskcue real-time use case is **unidirectional server→client**:

| Use case | Direction | Frequency | Source phase |
|---|---|---|---|
| Transcode progress | Server → Client | 1/sec during transcode | Phase 7 |
| Scan progress | Server → Client | 1/sec during scan | Phase 5 |
| Notification delivery | Server → Client | Rare (only on event) | Phase 13 |
| Session kicked (admin force-logout) | Server → Client | Rare | Phase 4 |
| Playback command (remote control) | Server → Client | Rare (only when remote triggers) | Phase 7/16 |
| Analytics dashboard live update | Server → Client | 1-5/sec while viewing | Phase 11 |
| Trust alert (impossible travel) | Server → Client | Rare | Phase 11 |

**Zero client→server push use cases exist.** The `can_remote_control` capability sounds bidirectional but isn't: the controlling client (e.g., phone) sends commands via standard REST `POST` endpoints; the server pushes the resulting command to the *target* client (e.g., TV) via SSE. The "remote" is the phone, not the SSE connection.

WebSocket's bidirectional capability is overhead Duskcue doesn't need. SSE matches the actual traffic shape exactly.

### SSE vs WebSocket — Detailed Comparison

| Concern | SSE | WebSocket | Winner for Duskcue |
|---|---|---|---|
| Directionality | Server→Client only | Bidirectional | SSE — all use cases are server→client |
| Browser API | `EventSource` (built-in) | `WebSocket` (built-in) | Tie |
| Auto-reconnect | Built into `EventSource` | Manual (library like `reconnecting-websocket`) | **SSE** |
| Missed-event recovery | Built-in via `Last-Event-ID` header | Must implement application-level replay | **SSE** |
| Authentication | HTTP — session cookies work natively | Browser API can't set headers; token must go in query string | **SSE** (avoids token-in-URL leak risk) |
| HTTP/2 multiplexing | Shares HTTP/2 connection with other requests | Opens separate TCP connection (doesn't benefit from H2) | **SSE** |
| Connection limit (HTTP/1.1) | Counts against 6-connection-per-origin limit | Doesn't count (separate protocol) | WebSocket (but moot on HTTP/2) |
| Proxy/CDN friendliness | Plain HTTP — works everywhere `Content-Type: text/event-stream` is understood | Requires HTTP Upgrade handshake support; some proxies block | **SSE** |
| Axum support | First-class: `axum::response::sse::{Event, Sse, KeepAlive}` | First-class: `axum::extract::ws` | Tie |
| Wire format | Human-readable text frames (`event:`, `data:`, `id:`, `retry:`) | Binary or text frames; application defines framing | SSE simpler |
| Compression | HTTP gzip/brotli applies | Per-message deflate extension (`permessage-deflate`) | Tie |
| Mobile background | OS kills connection when app backgrounded (same as WS) | Same | Tie — both need mobile push as fallback |
| Smart TV support | ✅ All Chromium-based (Tizen 6.0+/webOS 5.x+) | ✅ Same | Tie |

### Why Not Long-Polling

Long-polling (client `GET` with long timeout, server holds until data, client immediately re-`GET`s) was the pre-WebSocket fallback. It's strictly dominated by SSE for Duskcue's needs:

- Higher latency (request-response cycle for each event batch)
- Higher server load (connection churn, request parsing overhead)
- No standard missed-event recovery mechanism
- All browsers that Duskcue targets support SSE — no fallback needed

Long-polling is retained only as a documented "if a client's HTTP stack buffers SSE indefinitely" escape hatch (see Edge Cases), not as a primary transport.

### Why Not WebTransport

WebTransport (over HTTP/3) is the emerging high-performance bidirectional web transport. As of June 2026 it is **not viable for Duskcue**:

- ❌ No Safari support (desktop or iOS)
- ❌ No production-ready Rust server ecosystem (Axum/hyper have no stable WebTransport)
- ❌ Requires HTTP/3 end-to-end (Duskcue serves HTTP/1.1 + HTTP/2 today; HTTP/3 is a future Phase 15 consideration)
- ❌ Marginal benefit for unidirectional, low-frequency events like Duskcue's

WebTransport's wins (low-latency unreliable datagrams, bidirectional streams) target cloud gaming, collaborative editing, and live video — use cases Duskcue doesn't have. Revisit if Duskcue adds real-time sync (e.g., watch parties) where WebTransport's QoS matters.

## SSE Endpoint Design

### Endpoint

```
GET /api/v1/events
Accept: text/event-stream
```

Returns `Content-Type: text/event-stream` and holds the connection open, emitting events as they occur.

A single endpoint serves all event types. Clients filter to the events they care about via query parameter:

```
GET /api/v1/events?types=transcode_progress,scan_progress
```

When `types` is omitted, the client receives all event types it's authorized for.

### Authentication

SSE uses standard HTTP — the session cookie (`Cookie: session=<token>`) is sent automatically by the browser on the `EventSource` connection. No query-string token. No credential leakage in URLs/logs/proxy caches. This is the same auth path as every other authenticated endpoint.

The connection is authenticated once at handshake; subsequent events on the same connection are considered authenticated for that user. The server revalidates the session on reconnect.

**EventSource and the Authorization header:** The browser's native `EventSource` API cannot set custom headers (no `Authorization: Bearer ...`). Duskcue's web client uses HttpOnly session cookies (not bearer tokens) as the primary auth, so this is not a limitation. For non-browser clients (Tauri, Flutter) that prefer bearer tokens, use the [`@microsoft/fetch-event-source`](https://github.com/Azure/fetch-event-source) polyfill or an equivalent library that allows header customization. This is the standard pattern for SSE with bearer auth.

### Authorization

Event visibility is scoped to the authenticated user. A regular user receives only their own events (their transcode sessions, their notifications). An admin receives their own events plus any events targeting users they can manage (per `can_manage_users`, `can_manage_server` capabilities). The server enforces this at event-publish time, not at the transport layer — the SSE handler simply subscribes the connection to the user's authorized event channels.

### Wire Format

Each event follows the [SSE wire format](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events):

```
event: transcode_progress
id: 01950abc-7def-4012-9b6c-4f8d2e1a0001
data: {"session_id":"01950abc-...","progress":42.5,"speed":"2.1x","eta_seconds":300}

```

- `event:` — the event type (matches the `?types=` filter values)
- `id:` — opaque event ID used for `Last-Event-ID` replay (UUIDv7, naturally time-ordered)
- `data:` — JSON payload, UTF-8; may span multiple `data:` lines for multi-line content (rare for Duskcue)
- Trailing blank line — event delimiter

A `retry: 5000` field is sent once on connection open to suggest a 5-second reconnect delay if the connection drops.

### Heartbeat (KeepAlive)

Axum's `Sse::keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))` emits an SSE comment (`:keep-alive\n\n`) every 15 seconds when no events are flowing. This serves three purposes:

1. **Detect dead connections** — TCP keepalive alone is unreliable for detecting broken routes; application-level heartbeats surface dead connections faster so the server can free the subscriber slot
2. **Flush proxy/CDN buffers** — some proxies buffer responses; periodic writes flush them so genuine events reach the client promptly
3. **Defeat idle-connection timeouts** — nginx default `proxy_read_timeout` is 60 seconds; Cloudflare's free-tier idle timeout is 100 seconds. A 15-second heartbeat is comfortably below both

### Reconnection and `Last-Event-ID`

When the connection drops, the browser's `EventSource` automatically reconnects (after the `retry:` delay). It includes the last-received event ID in the `Last-Event-ID` request header.

The server maintains a short ring buffer (default: 100 events per user, ~5 minutes of activity) of recently-published events keyed by ID. On reconnect with `Last-Event-ID`, the server replays any events newer than that ID before subscribing to live events. This closes the "client briefly disconnected and missed events" gap.

**Event loss beyond the ring buffer is acceptable for Duskcue's use cases.** Transcode and scan progress events are overwriting state (latest value wins), so missing an old event is harmless. Notifications and trust alerts are also persisted in the database — clients fetch any missed notifications via REST on reconnect.

## Event Taxonomy

| Event Type | Source | Payload Schema Location | Notes |
|---|---|---|---|
| `transcode_progress` | `services/transcoding.rs` | [STREAMING.md](STREAMING.md) | 1/sec during active transcode; updates `TranscodeSession.progress` |
| `scan_progress` | `workers/library_scanner.rs` | [MEDIA_SCANNING.md](MEDIA_SCANNING.md) | 1/sec during active library scan |
| `storyboard_progress` | `workers/storyboard_generator.rs` | [STORYBOARDS.md](STORYBOARDS.md) | Emitted on admin-triggered generation (`phase: started|progress|completed`); scheduled task does not emit |
| `migration_progress` | `workers/migration_runner.rs` | [MIGRATIONS.md](MIGRATIONS.md) | Emitted to users with `can_manage_users` during import (`phase: started|importing|completed|failed|cancelled`) |
| `notification` | `services/notification_dispatch.rs` (Phase 13b Task 2) | [MOBILE_PUSH.md](MOBILE_PUSH.md) | New in-app notification created; published via `EventBus::publish()` on every dispatch |
| `tv_surface_changed` | `domains/tv/service.rs` plus playback/library/metadata/artwork/collection/access producers | [TV_PLATFORM_SURFACES.md](TV_PLATFORM_SURFACES.md) | User-scoped refresh hint for TV launcher/app rows; payload includes bounded reason, changed sections, affected IDs, `generated_after`, and optional `debounce_until` |
| `session_kicked` | `domains/auth/service.rs` | [AUTH.md](AUTH.md) | Admin force-logout; client must clear session and redirect to login |
| `playback_command` | `domains/playback/` | [STREAMING.md](STREAMING.md) | Server-initiated stop/pause (e.g., streaming policy auto-terminate) |
| `analytics_update` | Phase 11 analytics | Phase 11 (TBD) | Live dashboard refresh tick |
| `trust_alert` | Phase 11 security | [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md) | Impossible-travel detection fired |

Event types are named in `snake_case`. Future phases add new event types without breaking existing clients (clients ignore unknown event types per the SSE spec).

## Edge Cases

### Mobile OS Background Connection Killing

iOS and Android aggressively suspend background apps and close their network sockets. SSE connections — like WebSocket connections — die when the app is backgrounded. This is not solvable at the transport layer.

**Mitigation (Phase 16):** Mobile clients use OS-level push notifications (APNs for iOS, FCM for Android) for offline event delivery. The server's notification dispatch fan-outs to both SSE (for foreground web/desktop/TV clients) and the push gateway (for offline mobile clients). The push gateway is a Phase 16 concern; for now, mobile clients reconnect on app-foreground and fetch missed state via REST.

### Multi-Tab Connection Sharing (HTTP/1.1)

On HTTP/1.1, each browser tab opens its own SSE connection, which counts against the 6-connections-per-origin limit. Opening 6+ tabs to the same Duskcue server could starve other HTTP requests.

**Mitigation:** Serve over HTTP/2 (default whenever TLS is enabled — see [SECURITY.md](../security/SECURITY.md)). HTTP/2 multiplexes all requests over one connection, so SSE shares the pipe with regular API traffic and there is no per-tab connection cost. The HTTP/1.1 concern is moot on local (non-TLS) deployments where a single user rarely opens 6+ tabs.

**Future optimization:** Web clients can use the [BroadcastChannel API](https://developer.mozilla.org/en-US/docs/Web/API/BroadcastChannel) to elect a leader tab that holds the single SSE connection and relays events to other tabs. This is over-engineering for Duskcue's deployment scale and is not planned.

### Proxy/CDN/Enterprise Firewall Buffering

Some reverse proxies (notably nginx with default config, Cloudflare, and enterprise "antivirus" proxies) buffer HTTP responses. For SSE this is fatal — buffered events arrive in batches after long delays rather than streaming live.

**Mitigations:**

1. **Server emits `X-Accel-Buffering: no`** header on the SSE response — this is the nginx-specific escape hatch that disables buffering for that response. Axum handlers set this in the response builder.
2. **15-second KeepAlive heartbeat** — flushes buffers periodically even when no events are flowing
3. **Document exposed-mode proxy config** — operators deploying Duskcue behind nginx/Cloudflare in exposed mode must disable buffering for the `/api/v1/events` route. Example nginx config:

   ```nginx
   location /api/v1/events {
       proxy_pass http://duskcue;
       proxy_buffering off;
       proxy_cache off;
       proxy_set_header Connection '';
       proxy_http_version 1.1;
       chunked_transfer_encoding on;
   }
   ```

4. **Cloudflare-specific** — Cloudflare supports SSE but buffers by default on the free tier. Operators can either disable buffering via Cloudflare Rules or accept periodic heartbeat-paced delivery. Documented in [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) when Phase 15 lands.

### Antivirus Proxies That Buffer Indefinitely

Some enterprise antivirus proxies buffer the entire response body before forwarding — for SSE, this means the client never sees any events. The 15-second heartbeat typically defeats this (the proxy eventually gives up waiting for "the rest" of the response), but not always.

**Mitigation:** Document a fallback to REST polling for users in such environments. The current API_CONVENTIONS.md note "WebSocket events are supplementary — clients must not rely on WebSocket for critical state" carries forward to SSE: every event-stream use case is also achievable via REST polling, and clients SHOULD implement a polling fallback when SSE is unavailable.

This is also the only legitimate use case for the long-polling transport — if a deployment environment buffers SSE indefinitely but allows long-polling, a client may switch. This is explicitly a last resort and Duskcue does not implement long-polling server-side; affected clients fall through to REST polling at 5-second intervals.

### Event Ordering and Per-User Sequencing

Events for a single user are published via a single `tokio::sync::broadcast` channel per user, so they're naturally ordered per-user. There is no global ordering guarantee across users (and no use case requires one). The `id:` field is a UUIDv7 — naturally time-ordered, useful for `Last-Event-ID` replay only within a user's stream.

### Connection Limits per User

To prevent abuse (a malicious user opening thousands of SSE connections), the server enforces a per-user connection limit. Default: 5 concurrent SSE connections per user (covers reasonable multi-tab + multi-device scenarios). Excess connections receive HTTP 429 with the standard rate-limit Problem Details response. Configurable via `AuthConfig` (Phase 13 admin settings).

## Implementation Strategy

### Server (Rust / Axum)

- **`GET /api/v1/events`** handler returns `Sse<Stream>` with `KeepAlive::new().interval(15s)`
- **`AppState`** gains an `event_bus: Arc<EventBus>` field — a `DashMap<Uuid, broadcast::Sender<Event>>` keyed by user ID
- **Event publishing**: any domain code that needs to push an event calls `state.event_bus.publish(user_id, event)`. The `EventBus::publish` method sends to the user's broadcast channel (lazily created on first subscriber).
- **Subscription**: the SSE handler obtains the user's `broadcast::Receiver`, wraps it in a stream, applies the `?types=` filter, prepends any `Last-Event-ID` replay, and returns the `Sse` response
- **Heartbeat**: Axum's built-in `KeepAlive` handles this — no custom code
- **Replay ring buffer**: `EventBus` keeps a `VecDeque<Event>` per user (max 100, ~5 minutes of activity) protected by the same lock as the broadcast sender; on reconnect with `Last-Event-ID`, drains the buffer up to the named ID and emits those events first

### Client (SvelteKit Web)

- **`clients/web/src/lib/stores/events.js`** — new store managing the `EventSource` connection lifecycle
- Subscribes on login (`auth.init()` success), unsubscribes on logout
- Dispatches events to domain stores (`player.js` listens for `transcode_progress`, `libraries.js` listens for `scan_progress`/`storyboard_progress`, `notificationCenter.js` listens for `notification`, etc.)
- Reconnect is automatic (built into `EventSource`); store reflects connection state for UI ("live" vs "reconnecting" indicator in nav bar, optional)

### Client (Tauri Desktop)

- Reuses the web client's `events.js` via the embedded WebView
- For non-webview native UI surfaces (system tray notifications), Tauri can listen to the SSE via Rust-side `reqwest` and bridge events to the Tauri event bus (Phase 16 detail)

### Client (Flutter Mobile)

- Flutter maintains SSE only while the app is foregrounded and authenticated.
- The client must use a streaming HTTP implementation that can attach bearer auth headers, reconnect, and preserve `Last-Event-ID` where possible.
- On app resume, the client reconnects to SSE and refreshes notification unread count plus active playback/transcode state through REST if replay is unavailable.
- Backgrounded apps do not try to keep SSE alive. They rely on FCM/APNs/UnifiedPush and the in-app notification feed for missed events.
- Phase 16a implements these behaviors under the broader client decisions in [DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md).

## Comparison to Other Duskcue Transports

| Transport | Use case | Doc |
|---|---|---|
| **SSE** (`GET /api/v1/events`) | Server→client push of state changes | This document |
| **REST** (`GET/POST/...` `/api/v1/...`) | Request/response CRUD operations | [API_CONVENTIONS.md](API_CONVENTIONS.md) |
| **HTTP media stream** (`GET /api/v1/stream/{id}`) | Video/audio file bytes (Range requests) | [STREAMING.md](STREAMING.md) |
| **HLS** (`GET /api/v1/transcode/{id}/...`) | Adaptive bitrate streaming via FFmpeg | [STREAMING.md](STREAMING.md) |

SSE complements these — it carries metadata about state changes ("your transcode is 50% done"), while the actual media bytes flow over the dedicated streaming transports.

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| `/api/v1/ws` endpoint (previous design) | Superseded | Replaced by `/api/v1/events` SSE endpoint per this document |
| SSE endpoint `/api/v1/events` | ✅ Implemented | `server/src/services/events_handler.rs` — `GET /api/v1/events` with `?types=` filter, `Last-Event-ID` replay, `X-Accel-Buffering: no`, 15s KeepAlive, `retry: 5000` on open |
| `EventBus` in `AppState` | ✅ Implemented | `server/src/services/event_bus.rs` — `DashMap<Uuid, UserChannel>` with per-user `broadcast::Sender`, 100-event ring buffer, `ConnectionGuard` enforcing 5-connection-per-user limit |
| `Last-Event-ID` ring-buffer replay | ✅ Implemented | `EventBus::replay_after(user_id, last_id)` drains the per-user `VecDeque`; drained before live event subscription in the SSE handler |
| First consumer: `storyboard_progress` events | ✅ Implemented | `workers/storyboard_generator.rs` publishes `started`/`progress`/`completed` events for admin-triggered generation (per-library + per-item); scheduled-task invocation passes `None` for `requesting_user_id` (no SSE noise for background runs) |
| Per-user connection limit | ✅ Implemented | `EventBus::register_connection()` enforces `DEFAULT_MAX_CONNECTIONS_PER_USER = 5`; excess returns `AppError::RateLimited { code: "SSE_LIMIT_REACHED" }` |
| `tokio-stream` dependency | ✅ Added | `tokio-stream = { version = "0.1", features = ["sync"] }` — `BroadcastStream` wraps `broadcast::Receiver` as a `Stream` for Axum's `Sse` response |
| Svelte `events.js` store | ✅ Implemented | Phase 10 Task 12 — `clients/web/src/lib/stores/events.js`. Owns `EventSource` lifecycle; handler registry dispatches named events to domain stores. Layout connects on `$isAuthenticated`, disconnects on logout. `libraries.js` consumes `storyboard_progress` events. |
| `notification` SSE events | ✅ Wired by dispatch pipeline + consumed by web client | Phase 13b Task 2 — `services/notification_dispatch.rs` publishes `notification` events via `EventBus::publish()` on every dispatch. Payload includes `id`, `notification_type`, `category`, `priority`, `title`, `body`, `link`, and `created_at`. Phase 13b Task 6 — `clients/web/src/lib/stores/notificationCenter.js` subscribes via `events.on('notification', ...)` to prepend live notifications + increment unread count in the navbar bell. |
| `tv_surface_changed` SSE events | ✅ Implemented server-side | Phase 16b Tasks 9-10 — `domains/tv/service.rs` publishes bounded, debounced refresh hints for playback, watch-data, library scan/mutation, metadata/artwork refresh, poster/overlay, collection, access-control, and TV publication settings changes. TV clients consume this in platform phases. |
| SSE Prometheus metrics | ✅ Implemented | Pre-v1.0 Task 4 — `sse_connections`, `sse_connected_users`, `sse_connections_opened_total`, `sse_connections_rejected_total`, and `sse_events_published_total{event_type,delivered}` |
| Mobile push gateway (FCM/APNs) | Not implemented | Phase 16a |

The first concrete consumer of SSE is **storyboard generation progress** (Phase 10 Task 11) — admin clicks "Generate Storyboards" and sees per-file progress streamed to the libraries page. Transcode progress migration (Phase 7 follow-up) is the next consumer; the `Player.svelte` currently polls `GET /api/v1/playback/{session_id}` every few seconds.

### Architecture decisions (Phase 10 Task 11)

- **`EventBus` is a `services/` module, not a domain** — Cross-cutting infrastructure consumed by every domain (auth, playback, libraries, storyboards, notifications). Same convention as `encryption.rs`, `event_bus.rs` siblings. The SSE *transport* lives in `services/events_handler.rs`; the SSE *endpoint route* is registered in `router.rs` alongside `/health` and `/metrics`.
- **`UserChannel` lazily created and never removed** — `channel_for(user_id)` does a `DashMap::get` fast-path, falling back to `entry().or_insert()` for first-touch. A one-time active user never re-incurs the allocation; the memory cost is bounded by `CHANNEL_CAPACITY (256) + RING_BUFFER_CAPACITY (100)` events per user.
- **Per-connection task owns the `ConnectionGuard`** — The SSE handler spawns a `tokio::spawn`'d forwarder task that owns the broadcast receiver, replay drain, type-filter check, and the `ConnectionGuard`. When the client disconnects, Axum drops the response future → `ReceiverStream` sender closes → forwarder task exits on next `tx.send().await` → guard drops → connection count decrements. Deterministic, no leak window.
- **`BroadcastStream` lag handling** — If a subscriber falls >256 events behind, `broadcast::Receiver::recv()` returns `RecvError::Lagged`. The forwarder task logs at `debug` and continues; the client sees a brief gap. The 100-event ring buffer absorbs typical disconnect/reconnect windows without hitting this path.
- **`retry: 5000` on connection open** — Axum's `Event::default().retry(Duration::from_millis(5000))` emits the `retry:` SSE field once, suggesting a 5-second reconnect delay to the browser's `EventSource`. After the first event, only live/replayed events flow.
- **Replay strategy: drain the per-user ring buffer** — On reconnect with `Last-Event-ID: <uuid>`, the handler calls `EventBus::replay_after(user_id, id)` which returns all events strictly newer than the ID. UUIDv7 ids are time-ordered so the comparison is canonical. If the last-event-id is no longer in the buffer (older than ~5 minutes of activity), the entire buffer is returned — clients may receive redundant events, which is safe because progress events are idempotent overwrites and notifications carry their own `id` for client-side dedup.
- **`storyboard_progress` payload schema** — `{"phase":"started|progress|completed","library_id":null,"media_file_id":"uuid","media_item_id":"uuid|null","candidates":N,"processed":N,"generated":N,"errors":N}`. `phase` lets the client distinguish the initial fan-out (`started`, all zeros), per-file ticks (`progress`, incrementing counters), and the terminal state (`completed`, final counts).
- **Scheduled task does not emit SSE events** — `run_storyboard_generation()` (the scheduled 04:00 task) passes `None` for `requesting_user_id`. Rationale: there is no admin watching at 04:00; events would buffer into the ring buffer with no subscriber, wasting memory. Admin-triggered generation passes `Some(user_id)` from `Require<CanManageLibraries>::user.user_id`. The scheduled task's results are visible in the scheduled-task-run history via Phase 13a.
- **No background-task per event type** — Each domain worker that wants to push events simply calls `state.event_bus.publish(user_id, ServerEvent::new("type", payload))`. No registration, no trait wiring. New event types are documented in §Event Taxonomy but require no code changes to the bus or transport.
- **Prometheus labels stay bounded** — Pre-v1.0 Task 4 adds connection gauges and publish counters without user IDs, media IDs, URLs, or payload fields. `sse_events_published_total{event_type,delivered}` exposes fan-out behavior while keeping cardinality limited to documented event types and a boolean delivery result.

### Architecture decisions (Phase 10 Task 12)

- **Handler registry over `onmessage`** — The browser's `EventSource` only fires `onmessage` for events WITHOUT an `event:` field. Since Duskcue uses named events (`event: storyboard_progress`), the store uses `addEventListener(type, dispatcher)` per event type. A `Map<type, Set<handler>>` registry is the source of truth; `attachAllListeners()` re-registers dispatchers when a new `EventSource` is created (on reconnect after fatal error or logout/login). The dispatcher looks up handlers by type and calls them all, catching per-handler errors so one failing handler doesn't break others.
- **No `?types=` query filter** — The store connects to `/api/v1/events` without a type filter and dispatches client-side. Simpler than tracking which types are currently registered and reconnecting when the set changes. For Duskcue's scale (1–5 users per deployment), the bandwidth of receiving all authorized events is negligible. The server already enforces per-user authorization, so there's no security concern.
- **Native `EventSource` auto-reconnect** — The browser handles reconnection for network errors automatically. The store distinguishes two `onerror` cases: `readyState === CLOSED` → fatal HTTP error (401/403/429/500) → disconnect and update state to `'disconnected'`; `readyState === CONNECTING` → network error → browser is auto-reconnecting → update state to `'connecting'`. No custom exponential backoff — the server's `retry: 5000` field (sent on connection open) guides the browser's reconnect delay.
- **`Last-Event-ID` handled by the browser** — The store does NOT manually track `lastEventId` for replay. The browser's `EventSource` automatically sends the `Last-Event-ID` header on reconnect, and the server's `EventBus::replay_after()` handles the ring-buffer drain. The store does record `lastEventId` from each SSE event for diagnostics/UI, but it's not used for replay logic.
- **Layout-managed connection lifecycle** — The `+layout.svelte` connects/disconnects the SSE stream via `$effect(() => { if ($isAuthenticated) { events.connect(); return () => events.disconnect(); } })`. This is cleaner than having the events store import the auth store (which would create a circular dependency when domain stores register handlers via `events.on()` and also import auth for capability checks). The layout already manages auth redirects, so adding SSE lifecycle is one additional `$effect`.
- **Handler registration at module load** — Domain stores register their SSE handlers in the factory function (e.g., `libraries.js` calls `events.on('storyboard_progress', ...)` inside `createLibrariesStore()`). The handler is registered once at module evaluation time; the `on()` method adds to the registry even if the connection isn't open yet. When `connect()` is called later, `attachAllListeners()` registers all queued types as `EventSource` listeners. This avoids race conditions between handler registration and connection establishment.
- **`on()` returns an unsubscribe function** — Matches the Svelte store unsubscribe convention (`const unsub = events.on(type, fn); ... unsub()`). The libraries store handler is registered for the app lifetime (no unsubscribe), but the pattern is available for components that mount/unmount dynamically.
- **SSR-safe via `typeof EventSource` guard** — SvelteKit with `adapter-node` runs `+layout.svelte` on the server during SSR. The `connect()` method guards with `typeof EventSource === 'undefined'` to prevent SSR crashes. Additionally, domain store handler registration guards with `typeof window !== 'undefined'` since `EventSource` is only needed in the browser.
- **`storyboard_progress` dispatches to `libraries.js`** — The first consumer: the libraries store registers a handler that tracks progress in `storyboardProgress` state (set to the latest event payload; cleared on `phase: 'completed'`). A toast notification fires on completion (success for 0 errors, warning for >0 errors). Derived stores `storyboardProgress` and `isGeneratingStoryboards` are exported for UI consumption. Second consumer (Phase 13b Task 6): `notificationCenter.js` listens for `notification` events to prepend live notifications to the navbar bell dropdown + increment unread count. Future consumers: `player.js` for `transcode_progress`, `auth.js` for `session_kicked`.
- **No new npm dependencies** — The store uses Svelte's `svelte/store` (`writable`, `derived`) and the browser's native `EventSource` API. No `event-source-polyfill`, `reconnecting-event-source`, or similar libraries.

## Key Decisions

1. **SSE over WebSocket** — All Duskcue real-time use cases are unidirectional server→client. WebSocket's bidirectional capability is pure overhead. SSE matches the traffic shape exactly, with simpler auth (session cookies work natively), simpler Axum code (built-in `axum::response::sse`), and universal browser/TV support.
2. **Single endpoint with `?types=` filter** — One `GET /api/v1/events` serves all event types. Clients filter via query string. Simpler than per-domain SSE endpoints (one connection, one auth check, one reconnect path) and avoids the HTTP/1.1 6-connection limit.
3. **Session cookie auth, not query-string token** — The previous WebSocket design used `?token=...` query param auth because the WebSocket browser API can't set headers. SSE uses standard HTTP so the session cookie flows automatically. No credential leakage in URLs/logs.
4. **`Last-Event-ID` replay via per-user ring buffer** — 100 events (~5 min) per user. Covers brief disconnects (wifi handoff, tab sleep) without database queries. Events older than the buffer are recoverable via REST polling because they're overwriting-state (progress) or already-persisted (notifications).
5. **15-second KeepAlive heartbeat** — Well below nginx's 60s `proxy_read_timeout` and Cloudflare's 100s idle timeout. Flushes proxy buffers. Detects dead connections without waiting for TCP keepalive.
6. **`X-Accel-Buffering: no` for nginx operators** — Documented escape hatch for the most common proxy buffering issue. Operators in exposed mode must disable buffering on `/api/v1/events`; documented in deployment guide.
7. **REST polling is always a fallback** — Every SSE event carries information also available via REST. Clients SHOULD implement polling fallback (5-second interval) when SSE is unavailable. SSE is an optimization, not a critical-path dependency.
8. **No long-polling server-side** — Long-polling is strictly dominated by SSE for Duskcue's needs. Documented as a client-side fallback only (clients poll REST), not as a server transport.
9. **No WebTransport** — Not ready as of June 2026 (no Safari, no stable Rust server, requires HTTP/3). Revisit only if Duskcue adds latency-sensitive bidirectional features (watch parties, collaborative editing).
10. **Per-user connection limit (5)** — Prevents SSE-based DoS. Configurable via admin settings.

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | Authoritative API contract — the per-endpoint SSE contract lives there; this document explains the transport decision and strategy |
| [HTTP_CACHING.md](HTTP_CACHING.md) | Sister "transport strategy" doc — HTTP_CACHING covers request/response caching, this covers server→client push. Both are cross-cutting infrastructure decisions. |
| [STREAMING.md](STREAMING.md) | First SSE consumer — `transcode_progress` events. HLS video transport is separate from SSE. |
| [MEDIA_SCANNING.md](MEDIA_SCANNING.md) | Second SSE consumer — `scan_progress` events. |
| [AUTH.md](AUTH.md) | Session-cookie auth flows natively into SSE; `session_kicked` event source. |
| [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md) | `trust_alert` event source for impossible-travel detection (Phase 11). |
| [DOCKER_DEPLOYMENT.md](../operations/DOCKER_DEPLOYMENT.md) | Exposed-mode proxy config (nginx `proxy_buffering off` for `/api/v1/events`). |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | First consumer: Phase 7 (transcode progress); Phase 11 (analytics dashboard); Phase 13 (notifications); Phase 16 (mobile push gateway). |

## Research Sources

- **[RFC 8895](https://www.rfc-editor.org/rfc/rfc8895.html)** — Server-Sent Events (the wire format spec; technically the WHATWG HTML spec, but referenced as RFC 8895 in some places)
- **[WHATWG HTML §9.2 Server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html)** — The authoritative spec for `EventSource` and the `text/event-stream` format
- **[Mark Nottingham: Server-Sent Events, WebSockets, and HTTP](https://mnot.net/blog/2022/websockets)** — HTTP WG chair's analysis arguing SSE is the right pub/sub mechanism for the web, especially over HTTP/2
- **[MDN: Using Server-Sent Events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events)** — Reference for `EventSource` API, reconnection, `Last-Event-ID`
- **[web.dev: Real-time updates with Server-Sent Events](https://web.dev/articles/eventsource-basics)** — Practical guide and patterns
- **[RxDB: WebSockets vs SSE vs Long-Polling vs WebRTC vs WebTransport](https://rxdb.info/articles/websockets-sse-polling-webrtc-webtransport.html)** — Comprehensive comparison with latency/throughput analysis
- **[Axum SSE docs](https://docs.rs/axum/latest/axum/response/sse/index.html)** — `axum::response::sse::{Event, Sse, KeepAlive}` API
- **[Samsung Tizen Web Engine Specifications](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html)** — Per-year Tizen Chromium version (Tizen 6.0 = M76, all support EventSource)
- **[LG webOS Web API and Web Engine](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine)** — webOS TV Chromium history (all versions support EventSource)
- **[`@microsoft/fetch-event-source`](https://github.com/Azure/fetch-event-source)** — Standard polyfill for SSE with custom headers (Authorization) when needed by non-browser clients
