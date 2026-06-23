# Trakt.tv Integration — Domain Design

## Purpose

This document is the authoritative design for the Trakt.tv integration domain (`server/src/domains/trakt/`). It covers the HTTP API surface, OAuth device code flow, sync architecture, error handling, and configuration. The DB schema is defined in [DATABASE.md](DATABASE.md) (Trakt.tv Integration section); the error code registry is in [ERROR_HANDLING.md](ERROR_HANDLING.md) (TRAKT section).

## Overview

Trakt.tv is a first-class, per-user integration — not a plugin. Each Duskcue user can link their own Trakt account to sync watched history, watchlist, collection, and ratings bidirectionally. This supports local users, remote users, and shared users equally — each with their own Trakt identity.

The integration is a user-scoped resource: every endpoint requires `AuthenticatedUser`, and all queries are scoped to the requesting user's `user_id`. No admin capability is needed to manage one's own Trakt link.

## API Surface

All routes are under `/api/v1/trakt/*` per [API_CONVENTIONS.md](API_CONVENTIONS.md) route table.

### Account Linking (OAuth Device Code Flow)

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/api/v1/trakt/account` | Get linked account status (trakt username, token expiry, sync settings) |
| `POST` | `/api/v1/trakt/account/link` | Start device code flow — returns `device_code`, `user_code`, `verification_url` |
| `POST` | `/api/v1/trakt/account/poll` | Poll for device code completion — exchanges device code for access token |
| `DELETE` | `/api/v1/trakt/account` | Unlink account — deletes `trakt_accounts` row (cascades to `trakt_sync_state`) |

### Sync Settings

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/api/v1/trakt/settings` | Get per-category sync toggles (watched, watchlist, collection, ratings) |
| `PUT` | `/api/v1/trakt/settings` | Update per-category sync toggles |

### Sync Operations

| Method | Route | Purpose |
|---|---|---|
| `POST` | `/api/v1/trakt/sync` | Trigger a manual sync (push local → Trakt, pull Trakt → local) |
| `GET` | `/api/v1/trakt/sync/status` | Get last sync timestamp, item counts, error state |

### Synced Data (Read-Only Views)

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/api/v1/trakt/history` | List items in `trakt_sync_state` with pagination (offset) |
| `GET` | `/api/v1/trakt/ratings` | List rated items from `trakt_sync_state` where `rating IS NOT NULL` |

## OAuth Device Code Flow

The device code flow (RFC 8628) is the primary linking method for Duskcue because the server runs headless and the user authenticates via a separate device (phone/laptop browser). This is the same pattern used by the auth domain's device linking ([AUTH.md](AUTH.md)).

### Flow

1. **Start** — `POST /api/v1/trakt/account/link` calls Trakt `POST /oauth/device/code` with `{ client_id }`. Returns:
   ```json
   {
     "device_code": "<long-lived code>",
     "user_code": "ABC12345",
     "verification_url": "https://trakt.tv/activate",
     "verification_url_complete": "https://trakt.tv/activate?user_code=ABC12345",
     "expires_in": 600,
     "interval": 5
   }
   ```

2. **User authorizes** — The user visits `verification_url` (or `verification_url_complete` for a one-click QR-code link), logs into Trakt, and enters the `user_code`.

3. **Poll** — `POST /api/v1/trakt/account/poll` calls Trakt `POST /oauth/token` with `{ code: device_code, client_id, client_secret, grant_type: "urn:ietf:params:oauth:grant-type:device_code" }`. The client polls at the `interval` (default 5s):
   - HTTP 200 → success: `{ access_token, refresh_token, expires_in, created_at, scope, token_type }` — store in `trakt_accounts`
   - HTTP 400 `authorization_pending` → retry after `interval` seconds
   - HTTP 400 `slow_down` → increase interval by 5 seconds, retry
   - HTTP 400 `expired_token` → device code expired, user must restart
   - HTTP 400 `access_denied` → user denied the request

4. **Token storage** — On success, Duskcue calls Trakt `GET /users/settings` with the access token to get the `trakt_username` and `trakt_user_id`, then inserts a row into `trakt_accounts`.

### Token Refresh

Access tokens expire after **90 days (~3 months)** — Trakt returns `expires_in ≈ 7776000` seconds. (Confirmed June 2026 via Trakt maintainer `@tysonkerridge` in GitHub issue #48: *"you get an `expires_in` value of ~7776000 (seconds) which is 90 days aka ~3 months."*)

**Critical — refresh_token rotation.** Trakt (Doorkeeper-backed) **rotates the refresh_token on every refresh**: the old refresh_token is revoked and a new one is issued alongside the new access_token. Maintainer `@rectifyer`: *"It could be revoked by the user or revoked once it is used and a new access token + refresh token is generated."* This means:

- The service **must persist the new token pair** (`access_token` + `refresh_token` + `token_expires_at`) back to `trakt_accounts` after every successful refresh. Failing to do so permanently locks the account out (the stale refresh_token is invalid; only a re-link recovers it).
- A lazy "refresh on 401, read-only" pattern is unsafe — it can race and lose the rotated token. Duskcue uses **proactive refresh with write-back**: `ensure_valid_token()` refreshes when < 5 minutes remain and writes the new pair to the DB in the same call, before returning the token to the caller.

Refresh is performed via `POST /oauth/token` with JSON `{ refresh_token, client_id, client_secret, redirect_uri, grant_type: "refresh_token" }`. The `redirect_uri` must match one registered for the Trakt app (Trakt/Doorkeeper enforces this). Duskcue stores the operator-configured `redirect_uri` in `TraktConfig` (default `http://localhost:48027/trakt/callback`).

If refresh fails (refresh token revoked by user, or the stored token was already rotated and not persisted), the account is marked as needing re-link — the user sees `TRAKT_003 Token expired` and must re-link via the device code flow.

### Content-Type

All Trakt OAuth requests (`/oauth/device/code`, `/oauth/token`) use **`Content-Type: application/json`** — confirmed via a real HAR capture in GitHub issue #48. This differs from RFC 8628's `application/x-www-form-urlencoded` example; Trakt's implementation accepts JSON bodies for OAuth.

## Sync Architecture

Bidirectional sync between Duskcue's `user_item_data` and Trakt's sync endpoints. The sync worker (`server/src/workers/trakt_sync.rs`, Task 6) runs as a scheduled task every 30 minutes.

### Push (Duskcue → Trakt)

When a user marks an item watched, rates it, or adds it to a collection, the change is pushed to Trakt:
- **Watched** → `POST /sync/history` with `{ movies: [{ ids, watched_at }], episodes: [...] }`
- **Ratings** → `POST /sync/ratings` with `{ movies: [{ ids, rating, rated_at }], ... }`
- **Collection** → `POST /sync/collection` with `{ movies: [{ ids, collected_at }], ... }`

### Pull (Trakt → Duskcue)

On scheduled sync, Duskcue pulls the user's Trakt state:
- `GET /sync/watched/movies?page=N&limit=250` — paginated watched movies
- `GET /sync/watched/shows?page=N&limit=250&extended=progress` — paginated watched shows with season progress
- `GET /sync/watched/episodes?page=N&limit=250&extended=min` — compact watched episodes
- `GET /sync/watchlist` — watchlist items
- `GET /sync/collection` — collection items
- `GET /sync/ratings` — ratings

Pulled data is matched against `media_items` by `trakt_id` (primary), then `tmdb_id`/`imdb_id`/`tvdb_id` (fallback), then title+year (last resort). Matched items update `trakt_sync_state` rows and, if the local `user_item_data` is stale, propagate the watched/rating state.

### Merge Strategy

Per [DATABASE.md](DATABASE.md) `user_item_data` design:
- `is_watched` — logical OR (if either source says watched, it's watched)
- `play_count` — MAX (highest count wins)
- `resume_position_ms` — MAX (furthest progress wins; cleared to 0 if either says watched)
- `rating` — Trakt rating overrides local if `rated_at` is newer (per-item; user can override locally after)

### Pagination (June 2026 API changes)

Per Trakt API discussion #775 (April 2026, enforced after June 30, 2026):
- Watched endpoints (`/sync/watched/{type}`) require `page` + `limit` query params
- Max page size: 250 items
- Default page size without `limit`: 100 items
- New `episodes` type for watched episodes
- `extended=min` returns compact format (`{ "trakt_id": ["watched_at"] }`) for efficient syncing
- `extended=progress` required for season progress data (shows)
- Always paginate until empty array `[]`; never assume one request returns everything

### Rate Limiting

Per [DATABASE.md](DATABASE.md) Trakt API summary:
- **POST**: 1 request/second (authed) — sync pushes throttled to 1/sec
- **GET**: 1000 requests/5 minutes (authed) — sync pulls can burst
- Respect `Retry-After` header on 429 responses
- The sync worker uses a `governor` rate limiter (1 req/sec for POST, burst-controlled for GET)

## Error Handling

Per [ERROR_HANDLING.md](ERROR_HANDLING.md) TRAKT section:

| Code | HTTP | Meaning |
|---|---|---|
| `TRAKT_001` | 409 | Trakt account not linked |
| `TRAKT_002` | 429 | Trakt API rate limited |
| `TRAKT_003` | 409 | Trakt token expired (needs re-link) |
| `TRAKT_004` | 503 | Trakt API unavailable |
| `TRAKT_005` | 504 | Trakt API timeout |

Additional domain-specific variants (mapped to existing error codes, following the Segment/Storyboard precedent):
- `DeviceCodeExpired` → 400 BAD_REQUEST (OAuth device code expired)
- `DeviceCodePending` → 400 BAD_REQUEST (authorization pending — client should retry)
- `DeviceCodeDenied` → 403 FORBIDDEN (user denied authorization)
- `SyncInProgress` → 409 CONFLICT (a sync is already running for this user)
- `NotConfigured` → 500 INTERNAL (admin hasn't configured Trakt client_id/secret)
- `Database` → 500 INTERNAL (sqlx catch-all)

`TRAKT_004` (ServiceUnavailable) and `TRAKT_005` (Timeout) are included in the `Retry-After` header group per [ERROR_HANDLING.md](ERROR_HANDLING.md) reference implementation.

## Configuration

Trakt OAuth credentials (`client_id`, `client_secret`, `redirect_uri`) are operator-configured. The admin registers a Trakt app at `https://trakt.tv/oauth/applications` to obtain these. They are stored in `server_config.integrations.trakt` JSONB and encrypted at rest via the existing `EncryptionKey` (AES-256-GCM), following the same pattern as subtitle provider credentials (Phase 9 Task 8) and metadata provider keys (Phase 6 Task 13).

The `client_id` is public (sent to the browser during device code flow); the `client_secret` is server-side only and is the only field encrypted at rest (the `client_id` and `redirect_uri` are not secret). Both `client_id` and `client_secret` are required for the OAuth token exchange and refresh.

The `redirect_uri` is required only for the refresh-token grant (Trakt/Doorkeeper enforces that it matches an app-registered URI). It is not used in the device-code request or the initial device-code token exchange. Default `http://localhost:48027/trakt/callback`.

### Rust struct (`server/src/state.rs`)

```rust
pub struct TraktConfig {
    pub client_id: String,          // public; empty = not configured
    pub client_secret: String,      // encrypted at rest via EncryptionKey
    pub redirect_uri: String,       // default "http://localhost:48027/trakt/callback"
}
```

When `client_id` and `client_secret` are both empty, OAuth endpoints return `TRAKT NotConfigured` (mapped to 500 INTERNAL).

### Polling Model

The server performs **a single poll per client request**. `POST /api/v1/trakt/account/poll` makes exactly one `/oauth/token` attempt and returns:

- **Success (200)** → stores the token pair + account info, returns `TraktAccountResponse` with `linked: true`.
- **`authorization_pending`** → returns `DeviceCodePending` (400 BAD_REQUEST); the client retries after `interval` seconds.
- **`slow_down`** → returns `DeviceCodePending`; the client increases its local interval by 5 seconds.
- **`expired_token`** → returns `DeviceCodeExpired` (400); the user must restart the device code flow.
- **`access_denied`** → returns `DeviceCodeDenied` (403); the user denied authorization.

The client (web UI) drives the poll loop, not the server. This keeps HTTP connections short and matches the existing `DeviceCodeResponse` DTO contract.

## Scheduled Task

Per [DATABASE.md](DATABASE.md) scheduled tasks table:
- **Name**: `trakt_sync`
- **Interval**: 1800s (30 min)
- **Timeout**: 30 min
- **Config**: `{}` (empty — per-user sync settings come from `trakt_accounts` rows)

The worker iterates all `trakt_accounts` where `sync_enabled = true` and `token_expires_at > now()`, performing bidirectional sync for each user. Task 6 implements the worker; Task 3 (this scaffolding) registers the domain structure only.

## Implementation Status

| Component | Status | Phase |
|---|---|---|
| Domain scaffolding (five-file pattern) | ✅ Implemented | Phase 11 Task 3 |
| OAuth device code flow | ✅ Implemented | Phase 11 Task 4 |
| Token refresh (proactive, with write-back) | ✅ Implemented | Phase 11 Task 4 |
| Bidirectional sync | ⏳ Pending | Phase 11 Task 5 |
| Sync worker | ⏳ Pending | Phase 11 Task 6 |

### Task 4 — OAuth Implementation Notes

- **`services/trakt_client.rs`** — dedicated HTTP client module (same convention as `tvdb_client.rs`, `subdl_client.rs`). Owns a `reqwest::Client` (30s timeout, 10s connect, redirects disabled per API_SECURITY.md). Methods: `request_device_code()`, `exchange_device_code()`, `refresh_token()`, `get_user_settings()`. Each maps HTTP failures to `TraktError` (`ServiceUnavailable`/`Timeout`/`RateLimited`).
- **Proactive `ensure_valid_token()`** — service-layer function: loads `trakt_accounts`, refreshes when `token_expires_at - now() < 5 min`, writes the new pair back to the DB, then returns the access token. This is the safe pattern given refresh_token rotation.
- **`/users/settings` mapping** — `account.id` → `trakt_user_id` (BIGINT); `user.username` → `trakt_username` (TEXT). Stored alongside the token pair on successful device-code exchange.
- **`IntegrationsConfig` expanded** — `TraktConfig { client_id, client_secret, redirect_uri }` added under `integrations.trakt`. `client_secret` encrypted at rest; decrypted in `load_runtime_config()` via `decrypt_trakt_config()`.
- **Single-poll-per-request** — `poll_device_code()` performs one `/oauth/token` attempt and maps the RFC 8628 error strings (`authorization_pending`, `slow_down`, `expired_token`, `access_denied`) to `TraktError` variants. The client drives the retry loop.
- **Re-link as upsert** — `poll_device_code()` success does `INSERT ... ON CONFLICT (user_id) DO UPDATE` on `trakt_accounts` (the `user_id` UNIQUE constraint), so re-linking the same Duskcue user to a new Trakt account replaces the old row cleanly.

## Research Sources

- Trakt API Official Documentation: https://trakt.docs.apiary.io/
- Trakt API Source Code (GitHub): https://github.com/trakt/trakt-api
- Trakt API Pagination & Extended Defaults Discussion (GitHub #775, April-June 2026): https://github.com/trakt/trakt-api/discussions/775
- Trakt API Pagination & Sorting Updates Discussion (GitHub #681, January 2026)
- Trakt Forums — Updating Trakt Limits for 2026 (February 2026)
- Trakt Forums — Rate Limit Discussion (January 2025)
- Token TTL + refresh_token rotation (GitHub #48): https://github.com/trakt/trakt-api/issues/48 — confirmed 90-day `expires_in` (~7776000s) and that refresh_token rotates/revokes on each refresh (maintainer `@rectifyer`, `@tysonkerridge`)
- Refresh endpoint = `/oauth/token` (GitHub #173): https://github.com/trakt/trakt-api/issues/173 — confirmed refresh uses the same endpoint as the device-code token exchange with `grant_type: "refresh_token"` + `redirect_uri`
- OAuth refresh at scale (GitHub discussion #556): https://github.com/trakt/trakt-api/discussions/556 — confirmed device flow + refresh are subject to standard authed-POST rate limits
