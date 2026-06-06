# API Conventions

## Overview

This document defines the authoritative conventions for the server's REST API. Covers: URL structure, versioning, HTTP methods, request/response formats, pagination, rate limiting, authentication headers, filtering/sorting, async operations, conditional requests, CORS, and error response integration.

Application-layer security patterns (input validation, BOLA prevention, response DTO separation, SSRF prevention, request payload limits, admin endpoint isolation) are documented in [API_SECURITY.md](../security/API_SECURITY.md). This document covers API structure and conventions; that document covers API security against OWASP Top 10.

All clients — SvelteKit web, Tauri desktop, Flutter mobile, TV apps, and third-party integrations (Classifarr) — consume this API.

## URL Structure

### Base Path

All API endpoints are prefixed with `/api/v1/`.

```
/api/v1/{resource}
/api/v1/{resource}/{id}
/api/v1/{resource}/{id}/{sub-resource}
```

### Rules

| Rule | Example | Rationale |
|---|---|---|
| Plural nouns for collections | `/api/v1/libraries`, `/api/v1/media-items` | Consistent, predictable |
| Kebab-case for multi-word resources | `/api/v1/media-items`, `/api/v1/scheduled-tasks` | URL-safe, readable |
| UUIDv7 as resource identifier | `/api/v1/libraries/01950abc-def0-7000-8000-000000000001` | Matches PK strategy |
| Sub-resources limited to one level | `/api/v1/libraries/{id}/scan` | Avoid deep nesting |
| Actions as sub-resource verbs | `/api/v1/libraries/{id}/scan`, `/api/v1/media/{id}/refresh` | RPC-like operations that don't map to CRUD |
| No verbs in top-level paths | Not: `/api/v1/scan-library` | HTTP method is the verb |

### Route Pattern per Domain

Each domain module registers routes under `/api/v1/{domain-resource}`:

```rust
pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/api/v1/libraries", get(list).post(create))
        .route("/api/v1/libraries/{id}", get(get_one).patch(update).delete(delete))
        .route("/api/v1/libraries/{id}/scan", post(scan))
        .route("/api/v1/libraries/{id}/items", get(list_items))
        .with_state(state)
}
```

### Endpoint Inventory (by Domain)

| Domain | Base Path | Operations |
|---|---|---|
| Auth | `/api/v1/auth/*` | Registration, login, logout, passkey management, TOTP, device linking, re-auth |
| Users | `/api/v1/users` | CRUD, capabilities, library access, sessions |
| Libraries | `/api/v1/libraries` | CRUD, scan, refresh, items |
| Media | `/api/v1/media-items` | Get, search, refresh metadata, versions |
| Playback | `/api/v1/playback/*` | Start session, heartbeat, stop, bookmarks, playlists |
| Streaming | `/api/v1/stream/*` | Manifest, segments, direct play |
| Analytics | `/api/v1/analytics/*` | Dashboard, play history, bandwitch, transcode stats |
| Trakt | `/api/v1/trakt/*` | Link account, sync, history, ratings |
| System | `/api/v1/system/*` | Config, tasks, notifications, backups, health |
| Search | `/api/v1/search` | Full-text search across media |
| Quality | `/api/v1/quality/*` | Device profiles, capability wizard, telemetry |
| Subtitles | `/api/v1/subtitles/*` | List, upload, download, sync settings |
| Overlays | `/api/v1/overlays` | CRUD overlay definitions, apply, templates |
| Collections | `/api/v1/collections` | CRUD, sync, templates |
| Artwork | `/api/v1/artwork/*` | Upload, lock, select, refresh |

## API Versioning

### Strategy: URI Prefix

```
/api/v1/libraries
```

**Why URI prefix:**
- Already established in PROJECT_STRUCTURE.md
- Self-hosted context — we control both server and all clients, so version bumps are coordinated
- Simple for all client types (web, mobile, desktop, TV, curl)
- Cacheable by URL (CDN, reverse proxy, browser)
- No hidden version negotiation (what you see is what you get)

**Why not alternatives:**

| Alternative | Why Not |
|---|---|
| Accept header versioning | REST-pure but opaque; hard to debug in browser; self-hosted users expect simple URLs |
| Custom header (`X-API-Version`) | Non-standard; proxy stripping risk; hidden from browser devtools |
| No versioning (evolution) | No escape hatch for breaking changes in long-lived self-hosted deployments |
| Query string (`?version=1`) | Breaks caching; semantically wrong (same resource regardless of version) |

### Version Lifecycle

| Event | Action |
|---|---|
| New fields added to response | Non-breaking; no version bump |
| Field deprecated | Mark deprecated in OpenAPI; return field for 2 major versions; remove in v3+ |
| Breaking change (removed field, changed type) | New major version (`v2`); maintain `v1` for at least 12 months |
| Security fix | Backported to all supported versions |

**Current version:** `v1`. There is no v2 yet. When v2 is created, both `/api/v1/` and `/api/v2/` routes coexist. The router assembly in `router.rs` merges both version routers.

### Implementation

```rust
// router.rs
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(v1::auth::router(state.clone()))
        .merge(v1::users::router(state.clone()))
        .merge(v1::libraries::router(state.clone()))
        // ... all v1 domains
        // Future: .merge(v2::auth::router(state.clone()))
        .route("/health", get(health_check))
        .layer(middleware_stack())
}

// When v2 exists, domains are organized:
// server/src/domains/v1/auth/
// server/src/domains/v2/auth/
```

## HTTP Methods

| Method | Usage | Idempotent | Success Status |
|---|---|---|---|
| `GET` | Retrieve resource(s) | Yes | `200 OK` |
| `POST` | Create resource or trigger action | No | `201 Created` (resource) or `202 Accepted` (action) |
| `PATCH` | Partial update to existing resource | No | `200 OK` |
| `PUT` | Full replacement of existing resource | Yes | `200 OK` |
| `DELETE` | Remove resource | Yes | `204 No Content` |

### Method per Route Pattern

| Route | GET | POST | PATCH | PUT | DELETE |
|---|---|---|---|---|---|
| `/api/v1/libraries` | List all | Create new | — | — | — |
| `/api/v1/libraries/{id}` | Get one | — | Update | Replace | Delete |
| `/api/v1/libraries/{id}/scan` | — | Trigger scan | — | — | — |

### POST vs PATCH for Actions

- **POST** to a sub-resource URL = trigger an action (`/scan`, `/refresh`, `/sync`)
- **PATCH** to a resource URL = update fields (`{"name": "New Name"}`)
- **PUT** to a resource URL = full replacement (rare; used for config overwrite)

## Request & Response Format

### Content Type

All requests and responses use JSON:

```
Content-Type: application/json; charset=utf-8
```

The server does not support XML, form-encoded, or other formats.

### Date/Time Format

All timestamps are ISO 8601 with timezone (RFC 3339):

```json
{
    "created_at": "2026-05-31T14:30:00Z",
    "last_played": "2026-05-31T14:30:00.123Z"
}
```

### UUID Format

All IDs are UUIDv7 in standard hyphenated lowercase:

```json
{
    "id": "01950abc-def0-7000-8000-000000000001"
}
```

### Null Handling

- Omitted fields and `null` values are equivalent (clients must handle both)
- Response fields that have no value are omitted entirely (reduces payload)
- PATCH requests treat `null` as "remove this field" (JSON Merge Patch semantics per RFC 7396)

### Standard Response Envelope

Single resource:

```json
{
    "id": "01950abc-def0-7000-8000-000000000001",
    "name": "Movies",
    "type": "movie",
    "created_at": "2026-05-31T14:30:00Z"
}
```

Collection (paginated — see Pagination section):

```json
{
    "items": [
        { "id": "...", "name": "Movies", "type": "movie" },
        { "id": "...", "name": "TV Shows", "type": "tv" }
    ],
    "total": 42,
    "cursor": "eyJpZCI6IjAxOTUw...",
    "has_more": true
}
```

Action response (202 Accepted):

```json
{
    "status": "accepted",
    "job_id": "01950abc-def0-7000-8000-000000000002",
    "location": "/api/v1/system/tasks/01950abc-def0-7000-8000-000000000002"
}
```

## Pagination

### Strategy: Hybrid — Cursor Default, Offset Opt-In

**Cursor pagination** is the default for all collection endpoints. It provides constant-time performance at any page depth using our UUIDv7 primary keys.

**Offset pagination** is available for endpoints that need random page access (admin tables, settings lists) where datasets are small and predictable.

### Cursor Pagination (Default)

Uses keyset pagination on UUIDv7 primary keys. Since UUIDv7 is monotonically time-ordered, cursoring on `id` provides natural chronological ordering without composite indexes.

**Request:**

```
GET /api/v1/media-items?limit=20&cursor=eyJpZCI6IjAxOTUw...
```

| Parameter | Type | Default | Max | Description |
|---|---|---|---|---|
| `limit` | integer | 20 | 100 | Number of items to return |
| `cursor` | string (base64) | null | — | Opaque cursor from previous response |
| `order` | string | `"desc"` | — | `"asc"` or `"desc"` (chronological by UUIDv7) |

**Response:**

```json
{
    "items": [...],
    "cursor": "eyJpZCI6IjAxOTUw...",
    "has_more": true
}
```

| Field | Description |
|---|---|
| `items` | Array of resource objects |
| `cursor` | Opaque base64 string; pass as `?cursor=` in next request. Absent when `has_more` is `false` |
| `has_more` | `true` if more items exist beyond this page |

**How the cursor works:**

The cursor is a base64-encoded JSON object containing the last item's sort key(s). For default UUIDv7 ordering:

```json
{"id": "01950abc-def0-7000-8000-000000000001"}
```

Encoded: `eyJpZCI6IjAxOTUwYWJjLWRlZjAtNzAwMC04MDAwLTAwMDAwMDAwMDAwMSJ9`

Clients treat cursors as opaque strings. Never construct or modify cursors client-side.

**SQL generation:**

```sql
-- Default (desc order, most recent first)
SELECT * FROM media_items
WHERE id < decode_cursor($1)
ORDER BY id DESC
LIMIT $2;

-- Ascending (oldest first)
SELECT * FROM media_items
WHERE id > decode_cursor($1)
ORDER BY id ASC
LIMIT $2;
```

The `LIMIT` is `limit + 1` internally — the extra row determines `has_more` without a COUNT query.

**Multi-column sort cursors:**

When sorting by non-unique columns (e.g., `title`, `added_at`), the cursor includes the sort column + `id` as tiebreaker:

```json
{"added_at": "2026-05-30T10:00:00Z", "id": "01950abc-..."}
```

### Offset Pagination (Opt-In)

Available for admin/settings endpoints where random page access is needed and datasets are small (< 10,000 rows).

**Request:**

```
GET /api/v1/users?page=2&page_size=25
```

| Parameter | Type | Default | Max | Description |
|---|---|---|---|---|
| `page` | integer | 1 | — | 1-indexed page number |
| `page_size` | integer | 25 | 100 | Items per page |

**Response:**

```json
{
    "items": [...],
    "total": 142,
    "page": 2,
    "page_size": 25,
    "total_pages": 6
}
```

**When to use each:**

| Scenario | Pagination | Rationale |
|---|---|---|
| Media libraries (movies, episodes) | Cursor | Large datasets, infinite scroll, constant performance |
| Activity feed, play history | Cursor | Chronological, real-time appended |
| Search results | Cursor | Large result sets, deep pagination common |
| Admin user list | Offset | Small dataset, needs page numbers for UI |
| Scheduled tasks list | Offset | Small dataset, no deep pagination |
| Collection items | Cursor | Can be large, usually browsed sequentially |
| Notifications | Cursor | Chronological feed |

### Pagination Parameter Validation

| Condition | Response |
|---|---|
| `limit` > max (100) | `VALID_001` (422) with field error |
| `limit` < 1 | `VALID_001` (422) with field error |
| `page` < 1 | `VALID_001` (422) with field error |
| `page_size` > max (100) | `VALID_001` (422) with field error |
| Both `cursor` and `page` provided | `VALID_001` (422) — use one strategy |
| Malformed `cursor` (invalid base64) | `VALID_001` (422) — "Invalid cursor" |

## Filtering & Sorting

### Filtering

Query parameters for exact match filtering:

```
GET /api/v1/media-items?type=movie&genre=action&year=2025
```

Multi-value filters use comma-separated values:

```
GET /api/v1/media-items?genre=action,thriller&status=matched,confirmed
```

Common filter parameters:

| Domain | Filters |
|---|---|
| Media items | `type`, `genre`, `year`, `status`, `library_id`, `resolution`, `hdr` |
| Users | `status`, `role` |
| Play history | `user_id`, `media_type`, `date_from`, `date_to` |
| Collections | `type`, `library_id`, `builder` |

### Sorting

```
GET /api/v1/media-items?sort=added_at&order=desc
```

| Parameter | Values | Default |
|---|---|---|
| `sort` | Field name: `added_at`, `title`, `year`, `rating`, `runtime` | `added_at` |
| `order` | `asc`, `desc` | `desc` |

When `sort` is specified, cursor pagination uses a composite cursor (sort field + `id`). Available sort fields are defined per endpoint in OpenAPI docs.

### Full-Text Search

```
GET /api/v1/search?q=the+matrix&type=movie
```

Uses PostgreSQL `websearch_to_tsquery()` with field-weighted GIN index. Documented in DATABASE.md (full-text search cross-cutting concern).

## Rate Limiting

### Crate: governor v0.6

`governor` provides token-bucket rate limiting with burst support. Pure Rust, no external dependencies (no Redis), Tower middleware compatible with Axum. In-process rate limiting is sufficient for our self-hosted single-instance deployment model.

### Rate Limit Tiers

| Tier | Scope | Limit | Burst | Applies To |
|---|---|---|---|---|
| **Global** | Per IP | 100 req/min | 50 | All unauthenticated endpoints |
| **Auth** | Per IP | 10 req/min | 5 | `/api/v1/auth/*` (login, register, passkey, TOTP) |
| **Authenticated** | Per user | 300 req/min | 100 | All authenticated API endpoints |
| **Streaming** | Per session | 600 req/min | 50 | `/api/v1/stream/*` (segment requests ~1/6s) |
| **Admin** | Per user | 1,000 req/min | 200 | `/api/v1/system/*` (settings, tasks, backups) |
| **Device linking** | Per IP | 5 req/15min | 3 | `/api/v1/auth/device-link` |

### Response Headers

Every response includes rate limit headers:

```
X-RateLimit-Limit: 300
X-RateLimit-Remaining: 287
X-RateLimit-Reset: 1717156800
```

| Header | Description |
|---|---|
| `X-RateLimit-Limit` | Maximum requests allowed in the current window |
| `X-RateLimit-Remaining` | Requests remaining in the current window |
| `X-RateLimit-Reset` | Unix timestamp when the rate limit window resets |

### Rate Limit Exceeded (429)

When a client exceeds their rate limit:

```json
{
    "type": "/errors/rate_limited",
    "title": "RATE_LIMITED",
    "status": 429,
    "detail": "Rate limit exceeded. Retry after 45 seconds.",
    "trace_id": "abc-123-def-456"
}
```

Response includes:

```
HTTP/1.1 429 Too Many Requests
Retry-After: 45
X-RateLimit-Limit: 300
X-RateLimit-Remaining: 0
X-RateLimit-Reset: 1717156800
```

### Implementation

```rust
use governor::{Quota, RateLimiter, clock::DefaultClock, state::keyed::DefaultKeyedStateStore};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::net::IpAddr;

pub struct RateLimitState {
    pub ip_global: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    pub ip_auth: Arc<RateLimiter<IpAddr, DefaultKeyedStateStore<IpAddr>, DefaultClock>>,
    pub user_authenticated: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
    pub session_streaming: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
    pub user_admin: Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            ip_global: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(100).unwrap())
                    .allow_burst(NonZeroU32::new(50).unwrap())
            )),
            ip_auth: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(10).unwrap())
                    .allow_burst(NonZeroU32::new(5).unwrap())
            )),
            user_authenticated: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(300).unwrap())
                    .allow_burst(NonZeroU32::new(100).unwrap())
            )),
            session_streaming: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(600).unwrap())
                    .allow_burst(NonZeroU32::new(50).unwrap())
            )),
            user_admin: Arc::new(RateLimiter::keyed(
                Quota::per_minute(NonZeroU32::new(1000).unwrap())
                    .allow_burst(NonZeroU32::new(200).unwrap())
            )),
        }
    }
}
```

### Rate Limit Configuration

Rate limits are configured in `server_config.auth` JSONB and can be adjusted via admin API:

```json
{
    "rate_limits": {
        "global_per_minute": 100,
        "global_burst": 50,
        "auth_per_minute": 10,
        "auth_burst": 5,
        "authenticated_per_minute": 300,
        "authenticated_burst": 100,
        "streaming_per_minute": 600,
        "streaming_burst": 50,
        "admin_per_minute": 1000,
        "admin_burst": 200
    }
}
```

Changes take effect on next request (no restart required). Governor limiters are rebuilt from config on reload.

## Authentication Headers

### Two Authentication Methods

The server supports two authentication methods simultaneously. The server detects the client type based on the presence of headers.

| Method | Header | Best For | Mechanism |
|---|---|---|---|
| **Session cookie** | Cookie: `session=<token>` | Web client (SvelteKit) | HTTP-only, `SameSite=Strict`, `Secure` flag |
| **Bearer token** | `Authorization: Bearer <token>` | Mobile, desktop, API clients | Session token in header |

### Session Cookie Details

```
Set-Cookie: session=eyJ0eXAiOiJKV1QiLCJhbGc...; HttpOnly; Secure; SameSite=Strict; Path=/api; Max-Age=604800
```

| Property | Value | Rationale |
|---|---|---|
| `HttpOnly` | Always | JavaScript cannot read session cookie (XSS protection) |
| `Secure` | Production only | Set when `server_config.security.force_secure_cookies` is true or when accessed via HTTPS |
| `SameSite` | `Strict` | CSRF protection; cookie never sent on cross-origin requests |
| `Path` | `/api` | Cookie only sent on API requests, not static assets |
| `Max-Age` | 7 days (604800s) | Matches session expiry in `user_sessions` table |

### Bearer Token Details

```
Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGc...
```

The token is the same session token stored in `user_sessions`. No separate JWT signing — sessions are validated against the database.

### API Key Authentication

For third-party integrations (Classifarr) and long-lived programmatic access:

```
Authorization: Bearer mv_apikey-...
```

API keys are prefixed with `mv_apikey-` to distinguish them from session tokens. Validated against the `api_keys` table.

### Authentication Flow

```
1. Client sends request to /api/v1/auth/login (passkey challenge-response)
2. Server validates, creates user_sessions row, returns session token
3. Web client: server sets HttpOnly cookie via Set-Cookie header
4. Mobile/desktop: client stores token, sends in Authorization header
5. Subsequent requests: extractors.rs AuthenticatedUser extractor validates cookie or bearer token
6. Token rotation: session tokens are rotated on a configurable interval (default: never; can enable per-session rotation)
```

### Extractor Implementation

```rust
// extractors.rs
pub struct AuthenticatedUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
    pub capabilities: Vec<String>,
}

#[async_trait]
impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try cookie first, then Authorization header
        let token = extract_session_token(parts)?;
        let session = validate_session(&state.db, &token).await?;
        Ok(AuthenticatedUser {
            user_id: session.user_id,
            session_id: session.id,
            capabilities: session.capabilities,
        })
    }
}
```

## CORS

### Configuration

CORS is configured in `server_config.network` JSONB:

```json
{
    "cors_origins": ["http://localhost:5173"],
    "cors_enabled": true
}
```

### Behavior

| Mode | CORS Policy |
|---|---|
| Local access (localhost) | Permissive — `Access-Control-Allow-Origin: *` for same-origin requests |
| LAN access | Configured origins — admin specifies allowed origins |
| Remote access | Strict — only explicitly configured origins in `server_config.security.allowed_origins`; credentials allowed |

CORS configuration is part of the security domain. Full CORS policy by network tier documented in [SECURITY.md](../security/SECURITY.md). Rate limiting documented below.

### Implementation

Uses `tower-http` `CorsLayer`:

```rust
use tower_http::cors::{CorsLayer, Any};

fn cors_layer(config: &NetworkConfig) -> CorsLayer {
    let origins = config.cors_origins.iter()
        .filter_map(|o| o.parse().ok())
        .collect::<Vec<_>>();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(Any)
        .allow_headers(Any)
        .allow_credentials(true)
}
```

## Async Operations

Long-running operations return `202 Accepted` with a job tracking URL.

### Request

```
POST /api/v1/libraries/01950abc.../scan
```

### Response

```
HTTP/1.1 202 Accepted
Location: /api/v1/system/tasks/01950abc-def0-7000-8000-000000000002

{
    "status": "accepted",
    "job_id": "01950abc-def0-7000-8000-000000000002",
    "location": "/api/v1/system/tasks/01950abc-def0-7000-8000-000000000002"
}
```

### Job Status Polling

```
GET /api/v1/system/tasks/01950abc-def0-7000-8000-000000000002
```

```json
{
    "id": "01950abc-def0-7000-8000-000000000002",
    "task_type": "library_scan",
    "status": "running",
    "progress": 45,
    "started_at": "2026-05-31T14:30:00Z",
    "metadata": {
        "library_id": "01950abc-...",
        "files_found": 1234,
        "files_processed": 555
    }
}
```

| Status | Description |
|---|---|
| `pending` | Queued, not started |
| `running` | In progress (progress 0-100) |
| `completed` | Finished successfully |
| `failed` | Errored out (see `error` field) |

### Operations That Use 202

| Operation | Task Type |
|---|---|
| Library scan | `library_scan` |
| Library metadata refresh | `metadata_refresh` |
| Backup trigger | `manual_backup` |
| Collection sync | `collection_sync` |
| Overlay application | `overlay_application` |

## Conditional Requests

### ETag for Cache Validation

Media metadata endpoints return `ETag` headers:

```
GET /api/v1/media-items/01950abc...

HTTP/1.1 200 OK
ETag: "abc123def456"
Content-Type: application/json

{ "id": "01950abc...", "title": "The Matrix", ... }
```

Client revalidation:

```
GET /api/v1/media-items/01950abc...
If-None-Match: "abc123def456"

HTTP/1.1 304 Not Modified
ETag: "abc123def456"
```

The `304 Not Modified` response has no body — the client uses its cached copy.

**ETag generation:** SHA-256 hash of the JSON response body. Computed after serialization.

**Endpoints with ETag:**

| Endpoint | ETag Scope |
|---|---|
| `GET /api/v1/media-items/{id}` | Per-item metadata |
| `GET /api/v1/libraries/{id}` | Library config |
| `GET /api/v1/system/config` | Full server config |

Collection list endpoints do not use ETag (paginated responses change frequently).

### Cache-Control Headers

| Endpoint | Cache-Control | Rationale |
|---|---|---|
| Media item metadata | `private, max-age=300` | 5 minutes; per-user due to watch status |
| Library config | `private, max-age=60` | 1 minute; changes are rare but visible |
| Static artwork URLs | `public, max-age=86400` | 24 hours; artwork rarely changes |
| HLS segments | `no-cache` | Always revalidate for streaming session validity |
| Search results | `no-store` | Dynamic, personalized |

## WebSocket

WebSocket connections are used for real-time events that benefit from push delivery.

### Endpoint

```
WS /api/v1/ws
```

Authentication via query parameter (WebSocket doesn't support headers in browser API):

```
WS /api/v1/ws?token=eyJ0eXAiOiJKV1QiLCJhbGc...
```

### Event Types

| Event | Direction | Payload |
|---|---|---|
| `transcode_progress` | Server → Client | `{ session_id, progress, speed, eta_seconds }` |
| `scan_progress` | Server → Client | `{ library_id, files_found, files_processed, percent }` |
| `notification` | Server → Client | `{ type, title, message, action_url }` |
| `session_kicked` | Server → Client | `{ reason }` |
| `playback_command` | Server → Client | `{ command: "stop" \| "pause", reason }` |

WebSocket events are supplementary — clients must not rely on WebSocket for critical state. All data is also available via REST polling.

## Health Check

```
GET /health
```

Unauthenticated. Returns 200 with JSON:

```json
{
    "status": "healthy",
    "version": "0.1.0",
    "database": "connected",
    "uptime_seconds": 86400
}
```

This endpoint is NOT under `/api/v1/` — it's at the root level for load balancer and Docker HEALTHCHECK use (see Dockerfile in PROJECT_STRUCTURE.md).

## OpenAPI Documentation

The server exposes OpenAPI 3.1 specification at:

```
GET /api/v1/docs
```

This serves a Scalar (or Swagger UI) documentation page. The raw OpenAPI JSON is at:

```
GET /api/v1/openapi.json
```

OpenAPI spec is generated from Axum route definitions at build time. Each domain module's `types.rs` request/response DTOs have `#[derive(ToJsonSchema, ...)]` annotations.

## Error Response Integration

Error responses follow RFC 9457 Problem Details format as documented in [ERROR_HANDLING.md](ERROR_HANDLING.md). Rate limit errors use the same format with HTTP 429:

```json
{
    "type": "/errors/rate_limited",
    "title": "RATE_LIMITED",
    "status": 429,
    "detail": "Rate limit exceeded. Retry after 45 seconds.",
    "trace_id": "abc-123-def-456"
}
```

`RATE_LIMITED` is a generic error code for HTTP-layer rate limiting. Domain-specific rate limit errors (e.g., `AUTH_012`, `QUALITY_004`) are for business-logic rate limits within individual domains.

## Request ID (Trace ID)

Every request is assigned a unique trace ID. This ID is:

1. Included in the `trace_id` field of every error response
2. Included in all server-side structured log entries for the request
3. Propagated through the `tracing` span stack

If a client sends an `X-Request-ID` header, the server uses that value. Otherwise, a new UUIDv7 is generated. The trace ID is returned in the response header:

```
X-Request-ID: 01950abc-def0-7000-8000-000000000001
```

## Implementation Checklist

### Files to Create/Modify

| File | Change |
|---|---|
| `server/src/middleware.rs` | Add `rate_limit_middleware` using governor; add CORS layer; add request ID layer |
| `server/src/extractors.rs` | `AuthenticatedUser` extractor (cookie + bearer); `PaginationParams` extractor (cursor/offset); `RateLimitHeaders` response extractor |
| `server/src/router.rs` | Ensure all domain routers merged; rate limit tiers applied per route group |
| `server/src/error.rs` | Add `RATE_LIMITED` variant to AppError |
| `server/src/lib.rs` | Add governor `RateLimitState` to `AppState` |
| `server/src/config.rs` | Parse rate limit config from `server_config.auth` |
| Each domain's `types.rs` | Add `PaginationParams`, cursor encode/decode helpers |
| Each domain's `handlers.rs` | Apply pagination to list endpoints |

### Crate Dependencies

Add to `server/Cargo.toml`:

```toml
governor = "0.6"
nonzero_ext = "0.3"
```

No additional Tower crates needed — `tower-http` CORS is already in workspace dependencies.

## Research Sources

- Microsoft Azure Architecture Center — RESTful Web API Design Best Practices (May 2025): https://learn.microsoft.com/en-us/azure/architecture/best-practices/api-design
- Milan Jovanovic — Understanding Cursor Pagination and Why It's So Fast (February 2025): https://www.milanjovanovic.tech/blog/understanding-cursor-pagination-and-why-its-so-fast-deep-dive
- Design Gurus — API Pagination Guide: Cursor vs Offset vs Keyset for High-Scale Endpoints (April 2026): https://designgurus.substack.com/p/api-pagination-guide-cursor-vs-offset
- OneUptime — How to Implement Rate Limiting in Rust Without External Services (January 2026): https://oneuptime.com/blog/post/2026-01-07-rust-rate-limiting/view
- governor crate — Token-bucket rate limiter for Rust: https://docs.rs/governor
- DEV Community — The Burden of API Versioning: URI or Header (May 2026): https://dev.to/merbayerp/the-burden-of-api-versioning-uri-or-header-1meh
- Phil Sturgeon (APIs You Won't Hate) — API Versioning Has No Right Way (September 2017): https://medium.com/apis-you-wont-hate/api-versioning-has-no-right-way-f3c75457c0b7
- Microsoft — Data API Builder Cursor-Based Pagination: https://learn.microsoft.com/en-us/azure/data-api-builder/keywords/after-rest
- Microsoft Graph — Paging Microsoft Graph Data: https://learn.microsoft.com/en-us/graph/paging
- RFC 7396 — JSON Merge Patch: https://www.rfc-editor.org/rfc/rfc7396
- RFC 9457 — Problem Details for HTTP APIs: https://www.rfc-editor.org/rfc/rfc9457
