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
