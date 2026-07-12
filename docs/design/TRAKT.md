# Trakt.tv Integration — Domain Design

## Purpose

This document is the authoritative design for the Trakt.tv integration domain (`server/src/domains/trakt/`). It covers the HTTP API surface, OAuth device code flow, sync architecture, error handling, and configuration. The DB schema is defined in [DATABASE.md](DATABASE.md) (Trakt.tv Integration section); the error code registry is in [ERROR_HANDLING.md](ERROR_HANDLING.md) (TRAKT section).

## Overview

Trakt.tv is a first-class, per-user integration — not a plugin. Each Duskcue user can link their own Trakt account. Watched history sync is bidirectional; ratings and collection are currently pull-only. Watchlist remains a persisted preference without a sync implementation and is tracked as follow-up work. This supports local users, remote users, and shared users equally — each with their own Trakt identity.

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

### Operator Configuration

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/api/v1/settings/trakt` | Get masked Trakt application credentials (server administrators only) |
| `PUT` | `/api/v1/settings/trakt` | Update the client ID, client secret, or redirect URI (server administrators only) |

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

The current implementation pushes watched history. Ratings and collection push, along with watchlist pull/push, remain follow-up work. Duskcue batches the watched push into one POST; the available Trakt endpoints support one POST per category:
- **Watched** → `POST /sync/history` with `{ movies: [{ ids, watched_at }], episodes: [...] }` → 201 `{ added: { movies, episodes, shows, seasons }, not_found: { movies: [...], ... } }`
- **Ratings** → `POST /sync/ratings` with `{ movies: [{ ids, rating, rated_at }], ... }` → 201 `{ added: { ... }, not_found: { ... } }`
- **Collection** → `POST /sync/collection` with `{ movies: [{ ids, collected_at }], ... }` → 201 `{ added: {...}, existing: {...}, updated: {...}, not_found: {...} }`

**All three POST responses return `added` as an object** with per-type integer counts (movies/shows/seasons/episodes), not a flat integer. `existing`/`updated` appear **only** on the collection response. `watched_at`/`rated_at`/`collected_at` are optional in the request body (server uses the current time if omitted), but Duskcue always includes them to preserve the original timestamp. The `ids` object may use any ID namespace Trakt matches (trakt/imdb/tmdb/tvdb in that order), so Duskcue pushes whichever IDs the `media_item` has (`trakt_id` preferred, falling back to `tmdb_id`/`imdb_id`/`tvdb_id`). A non-empty `not_found` response is logged and fails the sync; affected local rows are not marked as confirmed, so a later run can retry safely.

### Pull (Trakt → Duskcue)

On scheduled sync, Duskcue pulls the user's Trakt state. As of June 2026, **all sync GET endpoints require pagination** (#775, #681): always send `page`+`limit` (max 250), loop until an empty array `[]` is returned. Do not rely on the `X-Pagination-*` headers for stop logic.

- `GET /sync/watched/movies?page=N&limit=250` — paginated watched movies (leaf granularity)
- `GET /sync/watched/episodes?page=N&limit=250` — paginated watched episodes (🆕 type, live since April 2026 per #775 maintainer reply). Episodes are the leaf granularity for TV, so Duskcue pulls episodes directly rather than flattening `shows` → `seasons` → `episodes`
- `GET /sync/watched/shows?page=N&limit=250` — optional aggregate show progress (`extended=progress`); used only for derived series-level status, not for leaf `user_item_data`
- `GET /sync/collection/{movies,shows}?page=N&limit=250` — paginated collection
- `GET /sync/ratings/{type}?page=N&limit=250` — paginated ratings (`type` = movies/shows/seasons/episodes)
- `GET /sync/watchlist/{type}?page=N&limit=250` — paginated watchlist

**Matching strategy.** Trakt returns every item with an `ids` object. Duskcue loads all `media_items` that carry an external ID once per sync into in-memory lookup maps keyed by `(type, trakt_id|tmdb_id|imdb_id|tvdb_id)`, then matches each Trakt item in priority order (`trakt` → `tmdb` → `imdb` → `tvdb`), scoped to the matching `media_items.type`. Unmatched items are skipped (logged at debug); title+year fuzzy matching is deliberately not used for sync (too error-prone for automated watched-state writes). Matched items upsert `trakt_sync_state` rows and propagate to `user_item_data` per the merge strategy.

**`ids` shapes per media type** (from the OpenAPI spec, verified against live #775 examples): movie = `{trakt, slug, imdb, tmdb}` (no tvdb); show = `{trakt, slug, imdb, tmdb, tvdb}`; episode = `{trakt, imdb, tmdb, tvdb}` (no slug; `tvdb` frequently null). Every id field is modelled `Option` to absorb nulls.

### Merge Strategy

Per [DATABASE.md](DATABASE.md) `user_item_data` design, on **pull** (Trakt → local) for each matched media item:
- `is_watched` — logical OR (if either source says watched, it's watched) — mirrors `upsert_user_item_data_stop` in the playback domain
- `play_count` — MAX (highest count wins; Trakt `plays` vs local `play_count`)
- `last_played_at` — MAX (Trakt `last_watched_at` vs local `last_played_at`)
- `resume_position_ms` — cleared to 0 if `is_watched` becomes true; otherwise left untouched (Trakt has no resume position)
- `rating` — `user_item_data.user_rating` is set **only when currently NULL** (Trakt rating applied to unrated items). Because `user_item_data` has no `rated_at` timestamp, Duskcue never overwrites an existing local rating on pull — the user's explicit local choice wins. This is a conservative simplification of the "Trakt rating overrides if newer" rule; a future migration adding a `user_rating_at` column to `user_item_data` would enable timestamp-based override.

On **push** (local → Trakt), Duskcue pushes `user_item_data` rows where `is_watched = true` but the corresponding `trakt_sync_state.is_watched` is false/absent (incremental push — only items not already confirmed on Trakt). Trakt's `add_to_history` is idempotent for existing history entries, so re-pushing is safe but wasteful; the `trakt_sync_state` check avoids redundant pushes.

### Pagination (June 2026 API changes)

Per Trakt API discussions #775 (April 2026, enforced after **June 30, 2026**) and #681 (January 2026, enforced from **June 15, 2026**):
- **All** sync GET endpoints (`watched`, `collection`, `ratings`, `watchlist`) require `page` + `limit` query params
- Max page size: **250** items (requesting more returns at most 250)
- Default page size without `limit`: 100 items (10 for watchlist)
- New `episodes` type for `/sync/watched` (leaf-level watched episodes, returns `{plays, last_watched_at, episode:{ids, season, number, ...}}`)
- `extended=min` returns a compact `{ "trakt_id": ["watched_at", ...] }` map (not used by Duskcue — we need the full ids object for matching)
- `extended=progress` required for season progress on `/sync/watched/shows` only
- **Loop until an empty array `[]`** is returned; never assume one request returns everything, and never assume the requested `limit` is the applied limit
- The `X-Pagination-Page`/`X-Pagination-Limit`/`X-Pagination-Page-Count`/`X-Pagination-Item-Count` headers exist but are inconsistently present on sync endpoints — Duskcue ignores them and uses the empty-array stop condition

### Rate Limiting

Per [DATABASE.md](DATABASE.md) Trakt API summary:
- **POST**: 1 request/second (authed) — sync pushes throttled to 1/sec
- **GET**: 1000 requests/5 minutes (authed) — sync pulls can burst
- Respect `Retry-After` header on 429 responses
- A 429 maps to `TraktError::RateLimited`; the API response preserves Trakt's `Retry-After` value when present and the scheduled worker stops the affected batch

## Error Handling

Per [ERROR_HANDLING.md](ERROR_HANDLING.md) TRAKT section:

| Code | HTTP | Meaning |
|---|---|---|
| `TRAKT_001` | 409 | Trakt account not linked |
| `TRAKT_002` | 429 | Trakt API rate limited |
| `TRAKT_003` | 409 | Trakt token expired (needs re-link) |
| `TRAKT_004` | 503 | Trakt API unavailable |
| `TRAKT_005` | 504 | Trakt API timeout |
| `TRAKT_006` | 409 | Trakt could not confirm every submitted item |
| `TRAKT_007` | 500 | Trakt token storage could not be secured |

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

The worker iterates all `trakt_accounts` where `sync_enabled = true`, performing the supported sync categories for each user. It deliberately does not filter by access-token expiry because `ensure_valid_token()` refreshes valid refresh tokens before the sync proceeds.

## Implementation Status

| Component | Status | Phase |
|---|---|---|
| Domain scaffolding (five-file pattern) | ✅ Implemented | Phase 11 Task 3 |
| OAuth device code flow | ✅ Implemented | Phase 11 Task 4 |
| Token refresh and encrypted account-token storage | ✅ Implemented | Phase 11 Task 4 + reliability follow-up |
| Sync settings / status / history / ratings views | ✅ Implemented | Phase 11 Task 5 |
| Watched sync engine (pull + confirmed push), ratings/collection pull | ✅ Implemented | Phase 11 Task 5 + reliability follow-up |
| Watchlist sync and ratings/collection push | ⏳ Not implemented | Trakt follow-up |
| Sync worker (scheduled task iteration and global failure result) | ✅ Implemented | Phase 11 Task 6 + reliability follow-up |
| Cross-instance sync lock, explicit pacing, and Trakt metrics | ⏳ Not implemented | Trakt follow-up |
| Dedicated admin credentials and personal Trakt settings surfaces | ✅ Implemented | Trakt follow-up |

### Reliability Follow-up Implementation Notes

- **Account-token encryption and compatibility migration** — `access_token` and `refresh_token` are now AES-256-GCM encrypted at write time. Existing plaintext rows are atomically upgraded on first use, so upgrades do not require an operator export or a forced re-link.
- **Refresh boundary corrected** — token refresh begins when expiry is at or within the five-minute safety buffer. Refreshed access and rotated refresh tokens are persisted together before another Trakt call can use them.
- **Fallback identifier persistence** — `trakt_sync_state.trakt_id` is nullable. This preserves sync state for media matched and submitted with TMDB, IMDb, or TVDB identifiers when Trakt's numeric identifier is unavailable.
- **Durable sync outcomes** — successful runs clear `last_sync_error` and write both timestamps; failed runs write a safe error code and attempt timestamp. The user-facing account and status responses expose these values without exposing tokens.
- **Honest task outcomes** — global Trakt failures now return an error to the scheduler rather than being recorded as a successful task. A manual sync response is completed synchronously and includes its summary.
- **Canonical web ownership** — `/admin/trakt` is the only operator credential editor; it keeps the stored secret masked unless replaced. `/settings/trakt` is user-scoped and drives device-code linking, supported category controls, manual sync, and status. The retired System integrations deep link redirects to the Admin surface.

### Task 4 — OAuth Implementation Notes

- **`services/trakt_client.rs`** — dedicated HTTP client module (same convention as `tvdb_client.rs`, `subdl_client.rs`). Owns a `reqwest::Client` (30s timeout, 10s connect, redirects disabled per API_SECURITY.md). Methods: `request_device_code()`, `exchange_device_code()`, `refresh_token()`, `get_user_settings()`. Each maps HTTP failures to `TraktError` (`ServiceUnavailable`/`Timeout`/`RateLimited`).
- **Proactive `ensure_valid_token()`** — service-layer function: loads `trakt_accounts`, refreshes when `token_expires_at - now() < 5 min`, writes the new pair back to the DB, then returns the access token. This is the safe pattern given refresh_token rotation.
- **`/users/settings` mapping** — `account.id` → `trakt_user_id` (BIGINT); `user.username` → `trakt_username` (TEXT). Stored alongside the token pair on successful device-code exchange.
- **`IntegrationsConfig` expanded** — `TraktConfig { client_id, client_secret, redirect_uri }` added under `integrations.trakt`. `client_secret` encrypted at rest; decrypted in `load_runtime_config()` via `decrypt_trakt_config()`.
- **Single-poll-per-request** — `poll_device_code()` performs one `/oauth/token` attempt and maps the RFC 8628 error strings (`authorization_pending`, `slow_down`, `expired_token`, `access_denied`) to `TraktError` variants. The client drives the retry loop.
- **Re-link as upsert** — `poll_device_code()` success does `INSERT ... ON CONFLICT (user_id) DO UPDATE` on `trakt_accounts` (the `user_id` UNIQUE constraint), so re-linking the same Duskcue user to a new Trakt account replaces the old row cleanly.

### Task 5 — Sync Implementation Notes

- **Pull granularity = leaf items (movies + episodes)** — Duskcue tracks `is_watched` at the leaf level (`media_items.type IN ('movie','episode')`). Series/season are containers, so Duskcue pulls `/sync/watched/movies` and the new `/sync/watched/episodes` type (live since April 2026 per #775) directly, avoiding the expensive `shows` → `seasons` → `episodes` flattening. `user_item_data` propagation only touches movie/episode rows.
- **In-memory matcher** — one query loads all `media_items` carrying any external ID (`SELECT id, type, trakt_id, tmdb_id, imdb_id, tvdb_id ...`), then four `HashMap<(MediaType, i64/String), Uuid>` maps answer matches in O(1). Priority order on collision: `trakt` → `tmdb` → `imdb` → `tvdb`. No title/year fuzzy matching for automated watched-state writes.
- **Pagination loop** — every sync GET iterates `page=1..` with `limit=250` until the response is an empty array. The page count is not parsed from headers (the `X-Pagination-*` headers are inconsistently present on sync endpoints per the OpenAPI spec); the empty-array stop is authoritative.
- **Rate limiting** — a `Retry-After` 429 maps to `TraktError::RateLimited`, preserves the retry value in the API response, and aborts the sync. Explicit client-side pacing remains follow-up work.
- **`run_sync(state, user_id)` is the single entry point** — performs pull → merge → watched push inside one logical operation, guarded by a per-process `DashMap` lock with a 15-minute TTL that returns `TraktError::SyncInProgress`. A PostgreSQL advisory lock is follow-up work for multi-instance deployments. `trigger_sync` calls it inline and returns a completed summary.
- **POST response shapes** — all three sync POSTs return `added` as an object `{movies, episodes, shows, seasons}` (per the OpenAPI spec — not a flat integer, even for ratings). For the implemented watched push, a non-empty `not_found` response fails the run and leaves local rows unconfirmed. `existing`/`updated` appear only on collection.
- **Conservative rating merge** — pull applies a Trakt rating to `user_item_data.user_rating` only when that column is NULL (no local rating timestamp exists to do timestamp-based override). Documented above in Merge Strategy.
- **`trakt_sync_state` upsert** — `INSERT ... ON CONFLICT (user_id, media_item_id) DO UPDATE` per matched item, keyed by the UNIQUE constraint. Stores the Trakt-side view (`trakt_id`, `is_watched`, `plays`, `rating`, `is_in_collection`, timestamps) so the next push can diff against it for incremental sends.
- **No new workspace dependencies** — sync uses the existing `reqwest`, `serde`, `sqlx`, `chrono`, `uuid` stack already wired in Task 4.

### Task 6 — Sync Worker Implementation Notes

- **Scheduled iteration over `run_sync`** — `workers/trakt_sync.rs::run_trakt_sync(state, task_id, config)` is a thin orchestration layer mirroring `subtitle_auto_fetch`, `segment_analysis`, and `storyboard_generation`: query candidate users → call `run_sync` per user → aggregate results. All pull/merge/push logic, token refresh, and per-user locking live in `run_sync` (Task 5); the worker adds only iteration, per-user error isolation, and aggregate logging.
- **Error classification: global abort vs per-user skip** — `NotConfigured`, `RateLimited`, `ServiceUnavailable`, and `Timeout` are global failures, so the worker aborts the batch and returns an error to the scheduler. `AccountNotLinked`, `TokenExpired`, `SyncInProgress`, `SyncIncomplete`, `TokenStorage`, and `Database` are recorded per user while iteration continues.
- **`token_expires_at > now()` guard intentionally omitted** — §Scheduled Task specifies `WHERE sync_enabled = true AND token_expires_at > now()`. The `token_expires_at` filter is NOT applied. `token_expires_at` tracks the *access* token (90-day TTL), but `ensure_valid_token` refreshes expired access tokens via the long-lived *refresh* token. A user whose access token lapsed but whose refresh token is valid would be incorrectly skipped forever. The candidate query filters only on `sync_enabled = true`; unrecoverable tokens surface as `TokenExpired` and are skipped per-user. This deviation is documented in the worker's module docs.
- **`ORDER BY last_full_sync_at ASC NULLS FIRST`** — users who have never synced (or synced longest ago) are processed first, so a backlog after server downtime clears fairly rather than always favoring the most-recently-synced user.
- **Optional `config.user_id` for single-user sync** — mirrors `segment_detector`'s `library_id` and `storyboard_generator`'s `library_id`. Enables targeted admin triggers and testing without iterating all users.
- **`trakt_sync` task enabled by default (no-op when unlinked)** — unlike `subtitle_auto_fetch` (disabled by default because it consumes external API quota unconditionally), `trakt_sync` is enabled by default because it is a pure no-op when zero `trakt_accounts` rows exist. The opt-in is at the account-linking level (`sync_enabled` per user), not the task level. Matches the original Phase 2 seed (`20260530070000_seed_default_data.sql`: `is_enabled = true`, `interval_seconds = 1800`).
- **Registered in `seed_default_tasks`** — added to `scheduler.rs::seed_default_tasks` alongside `subtitle_auto_fetch`, `segment_analysis`, and `storyboard_generation` for fresh-install consistency. The Phase 2 migration seed already creates the row for existing deployments, so no re-seed migration is needed (unlike segment_analysis/storyboard_generation which shipped dedicated re-seed migrations).
- **No new workspace dependencies** — the worker uses existing `sqlx`, `uuid`, and the Task 5 `run_sync` engine.

## Research Sources

- Trakt API Official Documentation: https://trakt.docs.apiary.io/
- Trakt API OpenAPI 3.0 spec (authoritative for exact request/response field shapes): https://api.apis.guru/v2/specs/trakt.tv/1.0.0/openapi.json
- Trakt API Source Code (GitHub): https://github.com/trakt/trakt-api
- Trakt API Pagination & Extended Defaults Discussion (GitHub #775, April-June 2026): https://github.com/trakt/trakt-api/discussions/775 — confirmed `/sync/watched/episodes` is live, pagination enforced after June 30 2026, and `added` is a per-type object on all sync POSTs
- Trakt API Pagination & Sorting Updates Discussion (GitHub #681, January 2026): max `limit` reduced to 250 on all paginated endpoints from June 15 2026
- Trakt Forums — Updating Trakt Limits for 2026 (February 2026)
- Trakt Forums — Rate Limit Discussion (January 2025)
- Token TTL + refresh_token rotation (GitHub #48): https://github.com/trakt/trakt-api/issues/48 — confirmed 90-day `expires_in` (~7776000s) and that refresh_token rotates/revokes on each refresh (maintainer `@rectifyer`, `@tysonkerridge`)
- Refresh endpoint = `/oauth/token` (GitHub #173): https://github.com/trakt/trakt-api/issues/173 — confirmed refresh uses the same endpoint as the device-code token exchange with `grant_type: "refresh_token"` + `redirect_uri`
- OAuth refresh at scale (GitHub discussion #556): https://github.com/trakt/trakt-api/discussions/556 — confirmed device flow + refresh are subject to standard authed-POST rate limits
