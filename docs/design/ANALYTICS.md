# Analytics Domain

## Overview

This document is the authoritative design for the analytics API — the read-side dashboard surface that surfaces play activity, bandwidth usage, concurrent streams, and security trust events to administrators.

The database schema for play sessions, play events, trust events, and trust scores is documented in [DATABASE.md](DATABASE.md) (Activity domain). The security analytics engine (GeoIP enrichment, impossible travel detection, false positive suppression) is documented in [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md). This document covers the HTTP API layer that reads from those tables.

---

## API Surface

All analytics endpoints live under `/api/v1/analytics/*` per [API_CONVENTIONS.md](API_CONVENTIONS.md) route table. Admin dashboard endpoints require the `can_view_analytics` capability (via `Require<CanViewAnalytics>` extractor).

### Route Table

| Method | Path | Capability | Purpose |
|---|---|---|---|
| `GET` | `/api/v1/analytics/overview` | `can_view_analytics` | Dashboard summary — total plays, unique users, watch time, concurrent streams, transcode breakdown |
| `GET` | `/api/v1/analytics/play-history` | `can_view_analytics` | Paginated play session list (cursor pagination, filterable by user/library/date/stream_decision) |
| `GET` | `/api/v1/analytics/top-media` | `can_view_analytics` | Most-watched media items by play count or watch duration |
| `GET` | `/api/v1/analytics/bandwidth` | `can_view_analytics` | Bandwidth usage time series (bucketed aggregation) |
| `GET` | `/api/v1/analytics/concurrent` | `can_view_analytics` | Current active/concurrent stream count and details |
| `GET` | `/api/v1/analytics/trust/scores` | `can_view_analytics` | Per-user trust scores |
| `GET` | `/api/v1/analytics/trust/events` | `can_view_analytics` | Trust event timeline (filterable by user/severity/acknowledged) |
| `POST` | `/api/v1/analytics/trust/events/{event_id}/acknowledge` | `can_view_analytics` | Mark a trust event as acknowledged |
| `GET` | `/api/v1/analytics/geoip/status` | `can_view_analytics` | GeoIP database file status (age, size, next update) |

### Query Parameter Conventions

Analytics endpoints share common query parameters for time range and filtering:

| Parameter | Type | Default | Description |
|---|---|---|---|
| `range` | string | `7d` | Time preset: `24h`, `7d`, `30d`, `90d`, `all` |
| `from` | ISO 8601 | — | Custom range start (overrides `range` when paired with `to`) |
| `to` | ISO 8601 | now | Custom range end |
| `user_id` | UUID | — | Filter to a specific user |
| `library_id` | UUID | — | Filter to a specific library |

When both `range` and `from`/`to` are provided, the explicit `from`/`to` pair takes precedence.

### Pagination

- **Play history** — cursor pagination (high-volume time-series table; `play_sessions` is range-partitioned by month and append-only). Uses the existing `PaginationParams` extractor's cursor mode with UUIDv7 natural time ordering.
- **Trust events** — offset pagination (lower volume; admin dashboard needs page numbers).
- **Top media / bandwidth / overview** — not paginated (aggregation results).

---

## Error Handling

Per [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md): "No new API error codes are needed — trust events are created in the background and surfaced via the admin dashboard, not as API errors."

The `AnalyticsError` enum defines domain-specific variants that map to **existing** error codes:

| Variant | Error Code | HTTP | When |
|---|---|---|---|
| `UserNotFound` | `USER_001` | 404 | Admin queries stats for a user_id that doesn't exist |
| `TrustEventNotFound` | `NOT_FOUND` | 404 | Trust event ID not found during acknowledge/detail |
| `InvalidDateRange` | `VALID_001` | 422 | `from` is after `to`, or dates are unparseable |
| `InvalidTimePreset` | `VALID_001` | 422 | `range` value is not one of the allowed presets |
| `Database` | `INTERNAL` | 500 | sqlx error catch-all |

This follows the precedent set by the segments and storyboards domains (domain-specific enum variants mapping to existing error codes).

---

## Implementation Notes

### Source Module Layout

```
server/src/domains/analytics/
├── mod.rs           # Module declarations + router assembly (9 routes)
├── handlers.rs      # 9 handlers wired to Require<CanViewAnalytics> + extractors
├── service.rs       # Service functions (todo!() stubs in Task 1, implemented in Task 2)
├── error.rs         # AnalyticsError enum (5 variants)
└── types.rs         # Row types, query types, response DTOs
```

The GeoIP service (`server/src/services/geoip.rs`) and impossible travel detection are separate service modules built in Tasks 7–9, not part of the analytics domain five-file pattern. The analytics domain's `geoip/status` endpoint reads the GeoIP service's state for the dashboard.

### Partition Awareness

`play_sessions` and `play_events` are range-partitioned by month. Queries should always filter on `started_at` / `event_at` to enable partition pruning. The time range query parameters are not just for display — they are essential for query performance on partitioned tables.

### Trust Score Decay

Trust score recovery (`+1 per day` of normal activity) is a background concern handled by a scheduled task (future), not by the analytics read API. The analytics API only reads the current `user_trust_scores` snapshot.

---

## Implementation Notes — Task 2 (Dashboard)

Task 2 implements the five dashboard service functions: `get_analytics_overview`, `list_play_history`, `get_top_media`, `get_bandwidth_usage`, `get_concurrent_streams`.

### Time Range Resolution (`resolve_time_range`)

A shared helper resolves the `range`/`from`/`to` query parameters into `(Option<DateTime<Utc>>, DateTime<Utc>>)` = `(from, to)`:

- `to` defaults to `Utc::now()` when not provided.
- If `from` is provided: validated `from <= to` (else `InvalidDateRange`); lower bound = `Some(from)`.
- Else: `range` preset (default `7d`) resolved to a lower bound; validated against `VALID_TIME_PRESETS` (else `InvalidTimePreset`). `all` → `None` (unbounded).
- Per the Query Parameter Conventions table, explicit `from`/`to` takes precedence over `range`.

All range-bound queries bind `from` via the static-SQL pattern `AND ($N::timestamptz IS NULL OR started_at >= $N)`, keeping `started_at` (the partition key) in the `WHERE` clause for partition pruning on the range-partitioned `play_sessions` table.

### Bucket-Interval Resolution (`resolve_bucket_interval`)

Bandwidth time-series bucketing adapts the bucket stride to the selected range so the chart always renders a bounded number of points:

| Range | Bucket stride | Points |
|---|---|---|
| `24h` | 1 hour | ~24 |
| `7d` | 6 hours | ~28 |
| `30d` | 1 day | ~30 |
| `90d` | 1 day | ~90 |
| `all` | 1 day (range clamped to 90d) | ~90 |

The bandwidth endpoint requires a concrete lower bound for `generate_series`. When the range is `all` (unbounded), the bandwidth query clamps the effective range to the last 90 days so the series has a finite axis; this is documented because an unbounded `generate_series` is undefined.

### Bandwidth Time Series — `generate_series` + `date_bin` + `LEFT JOIN` + `COALESCE`

Per current PostgreSQL best practice (Crunchy Data, Paul Ramsey), the bandwidth query produces a gap-free chart axis:

```sql
WITH buckets AS (
    SELECT generate_series($2, $3, $1::interval) AS bucket_start
),
agg AS (
    SELECT date_bin($1::interval, started_at, $2) AS bucket_start,
           COALESCE(SUM(bandwidth_bps), 0) AS bandwidth_bps,
           COUNT(*) AS session_count
    FROM play_sessions
    WHERE started_at >= $2 AND started_at <= $3
      AND ($4::uuid IS NULL OR user_id = $4)
      AND ($5::uuid IS NULL OR library_id = $5)
    GROUP BY 1
)
SELECT b.bucket_start, COALESCE(a.bandwidth_bps, 0), COALESCE(a.session_count, 0)
FROM buckets b LEFT JOIN agg a ON a.bucket_start = b.bucket_start
ORDER BY b.bucket_start
```

The bucket stride is bound as a parameter (`$1::interval`) — a value parameter, not SQL structure — so the query remains a static string satisfying sqlx 0.9's `SqlSafeStr` requirement. `date_bin` (PG14+) aligns session timestamps to the same stride/origin as `generate_series`, guaranteeing the `LEFT JOIN` matches. `COALESCE` fills empty buckets with zero so charts render without dead zones.

**Bandwidth semantics:** `bandwidth_bps` is a per-session point estimate recorded at session time. Each bucket sums the `bandwidth_bps` of sessions that *started* in that bucket and counts those sessions. This is a "bandwidth demand by start time" aggregation — the standard media-server dashboard semantics (Plex/Jellyfin). Concurrent-bandwidth-over-time (range-overlap integration) is intentionally deferred as it requires expensive overlap queries against a point estimate that isn't a sustained rate.

### Cursor Pagination — Play History

`list_play_history` uses the same cursor pattern as the media domain: base64-encoded `{"id":"<uuid>"}` JSON, `LIMIT N+1` for `has_more` detection, `WHERE id < cursor` for pagination. `play_sessions.id` is UUIDv7 (naturally time-ordered), so `ORDER BY id DESC` gives reverse-chronological order without a separate sort column. `stream_decision` is an additional filter bound via `($N::text IS NULL OR stream_decision = $N)`.

### Concurrent Streams

`get_concurrent_streams` selects sessions where `stopped_at IS NULL`, restricted to `started_at > now() - interval '24 hours'`. The 24-hour guard prunes to at most two partitions and excludes stale crash-recovery sessions (an unstopped session older than 24h is an artifact, not a real concurrent stream). `count` is derived from the result-set length; per-stream rows join `users.display_name` and `media_items.title` for the dashboard detail list.

### Transcode Breakdown (Overview)

The overview computes the stream-decision breakdown directly from the `play_sessions.stream_decision` column (`COUNT(*) FILTER (WHERE stream_decision = 'direct_play')`), not from `metadata` JSONB — `stream_decision` is a real NOT NULL column with a CHECK constraint. This differs from the quality domain's `get_transcode_breakdown` (Phase 7 Task 6) which queries `metadata->>'playback_type'`; that query targets a different (older) schema assumption and is not used by the analytics dashboard.

### Display-Name / Title Enrichment

Play-history and concurrent-stream queries use `LEFT JOIN users` / `LEFT JOIN media_items` with `COALESCE` fallbacks. Because `play_sessions` has `ON DELETE CASCADE` on both FKs, a deleted user or media item removes the session entirely, so `INNER JOIN` would be functionally equivalent; `LEFT JOIN` is defensive against partial-state edge cases and never drops analytics rows.

---

## Implementation Notes — Task 7 (GeoIP Status Endpoint)

Task 7 implements the `GET /api/v1/analytics/geoip/status` endpoint, which was a `todo!()` stub after Task 2 (Task 2 implemented only the 5 dashboard endpoints; the 4 security-analytics endpoints — trust/scores, trust/events, acknowledge, geoip/status — remained stubs for Tasks 7–9).

The endpoint delegates to the cross-cutting `GeoIpService` (`server/src/services/geoip.rs`) rather than querying the database directly — the GeoIP status is filesystem state (MMDB file presence, size, age) plus the in-memory reader's loaded/unloaded state, not database state. The handler reads `geoip_enabled` from `RuntimeConfig.analytics` and passes it alongside the `GeoIpService` reference to `service::get_geoip_status()`, which calls `GeoIpService::status()` to read the MMDB file metadata from disk.

The `GeoIpService` itself, the impossible-travel detection engine, and the weekly MMDB updater are documented in [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md) — they are security-domain infrastructure, not analytics-API concerns. The analytics domain only surfaces the status; the enrichment pipeline (populating `play_sessions.geo_*` columns) and trust-event creation land in Task 8.

---

## Implementation Notes — Task 8 (Trust Events + Impossible Travel)

Task 8 implements the remaining 3 security-analytics endpoints (`list_trust_scores`, `list_trust_events`, `acknowledge_trust_event`) — all were `todo!()` stubs since Task 1. It also adds the trust engine that populates the data these endpoints read.

### Trust Event CRUD

- **`list_trust_scores`** — `SELECT` from `user_trust_scores` joined to `users` for display name; ordered by score ASC (lowest trust first) then `last_violation_at DESC NULLS LAST`. No pagination — the dataset is bounded by the number of users.
- **`list_trust_events`** — offset pagination over `user_trust_events` joined to `users`; filterable by `user_id`, `severity`, and `acknowledged` via the standard `($N::T IS NULL OR column = $N)` pattern. `total_pages` computed via `div_ceil`. Ordered `created_at DESC`.
- **`acknowledge_trust_event`** — `UPDATE ... SET acknowledged = true, acknowledged_at = now() WHERE id = $1 RETURNING ...`; returns `TrustEventNotFound` when the event doesn't exist. Idempotent — acknowledging an already-acknowledged event is a no-op that refreshes the timestamp.

### Trust Engine (Detection)

The impossible-travel detection engine and the play-session geo enrichment pipeline live in `domains/analytics/service.rs` (not a separate `services/` module) because they are tightly coupled to the analytics domain's DB tables. See [ANALYTICS_SECURITY.md](../security/ANALYTICS_SECURITY.md) Task 8 Implementation Notes for the full design rationale, including: fire-and-forget enrichment via `tokio::spawn`, Haversine as pure `f64` math, `INET` column string handling, distance-based severity proxy, and `ConnectInfo` availability.
