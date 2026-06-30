# Notifications Domain Design

## Overview

This document is the authoritative design for the **in-app notification center REST API** — the user-facing CRUD surface for reading, acknowledging, and managing notifications. It complements [MOBILE_PUSH.md](MOBILE_PUSH.md) which is the authoritative design for the multi-channel *dispatch pipeline* (in-app + SSE + webhook + push fan-out).

The notifications domain covers:

- Notification list with cursor pagination + filtering (unread, type, category, priority)
- Mark-as-read (single + bulk "mark all")
- Delete (single + bulk "delete all read")
- Unread count (cheap badge query)
- Notification types listing (read-only reference data)
- User notification preferences (per-user per-type channel opt-in/out)
- Admin-only "send test notification" endpoint (Phase 13b verification flow)

Notifications are **created** by the dispatch pipeline (`services/notification_dispatch.rs`), not by direct API calls from users. The CRUD surface is read/update only — there is no user-facing "create notification" endpoint. The admin-only test endpoint exists to support the Phase 13b verification flow ("Admin triggers a test notification").

## API Surface

### Route Table

| Method | Path | Capability | Description |
|---|---|---|---|
| `GET` | `/api/v1/notifications` | `AuthenticatedUser` | List the current user's notifications (cursor pagination, filters) |
| `GET` | `/api/v1/notifications/unread-count` | `AuthenticatedUser` | Cheap unread count for badge display |
| `POST` | `/api/v1/notifications/{id}/read` | `AuthenticatedUser` | Mark single notification as read |
| `POST` | `/api/v1/notifications/read-all` | `AuthenticatedUser` | Mark all of the user's notifications as read |
| `DELETE` | `/api/v1/notifications/{id}` | `AuthenticatedUser` | Delete a single notification (user-owned) |
| `DELETE` | `/api/v1/notifications/read` | `AuthenticatedUser` | Delete all read notifications for the user (cleanup) |
| `GET` | `/api/v1/notification-types` | `AuthenticatedUser` | List notification types (reference data for preferences UI) |
| `GET` | `/api/v1/user/notification-preferences` | `AuthenticatedUser` | List the user's per-type channel preferences (with defaults) |
| `PUT` | `/api/v1/user/notification-preferences/{type_id}` | `AuthenticatedUser` | Upsert per-type channel preferences |
| `POST` | `/api/v1/notifications/test` | `Require<CanManageServer>` | Admin-only test notification dispatch (Phase 13b verification) |

### Authorization Model

All endpoints are **user-scoped** (the `user_id` from `AuthenticatedUser` is bound into every SQL `WHERE` clause). No capability check is needed for the user-scoped routes because BOLA prevention is enforced at the query layer — a user can only ever read/modify their own notifications. This matches the bookmarks and playlists pattern from the playback domain.

The single admin-only endpoint (`POST /api/v1/notifications/test`) uses `Require<CanManageServer>` because test notifications are an admin/operator verification tool, not a user feature.

### Pagination Strategy

Per [API_CONVENTIONS.md](API_CONVENTIONS.md) Pagination Strategy table — Notifications use **cursor pagination** because:

1. Notifications are a chronological feed (new rows appended over time)
2. The `notifications.id` column is `UUID DEFAULT uuidv7()` — UUIDv7 embeds a Unix-millisecond timestamp, so `id` is naturally time-ordered. Cursor pagination on `id` gives chronological ordering without a separate sort column.
3. The existing partial index `idx_notifications_unread ON notifications (user_id, created_at DESC) WHERE is_read = false` supports efficient unread retrieval.

Cursor format: base64-encoded JSON `{"id":"<uuid>"}`. `LIMIT N+1` pattern for `has_more` detection. The cursor is the `id` of the last item in the current page; the next page fetches `WHERE id < cursor ORDER BY id DESC` (default order) or `WHERE id > cursor ORDER BY id ASC` (if `order=asc`).

### Filtering

| Filter | Type | Values | Default |
|---|---|---|---|
| `is_read` | bool | `true`/`false` | None (all notifications) |
| `category` | string | `media`, `system`, `security`, `user`, `task` | None |
| `priority` | string | `low`, `medium`, `high` | None |
| `type` | string | notification_type.name (e.g., `new_media_added`) | None |

Multi-value filters are not supported (single value per filter). Categories and priorities are small enumerations validated against DB CHECK constraints.

## Error Handling

The notifications domain reuses existing error codes per [ERROR_HANDLING.md](ERROR_HANDLING.md):

| Domain Error Variant | Mapped Code | HTTP | Description |
|---|---|---|---|
| `NotificationsError::NotFound` | `SYS_004` | 404 | Notification not found / not owned by user |
| `NotificationsError::NotificationTypeNotFound` | `NOT_FOUND` | 404 | Notification type lookup failed |
| `NotificationsError::InvalidCategory` | `VALID_001` | 422 | Invalid category filter value |
| `NotificationsError::InvalidPriority` | `VALID_001` | 422 | Invalid priority filter value |
| `NotificationsError::InvalidChannelConfig` | `VALID_001` | 422 | Channel preference payload invalid |
| `NotificationsError::Database` | `INTERNAL` | 500 | SQL error catch-all |

`SYS_004` is already registered for "Notification not found" (per ERROR_HANDLING.md SYS section). The notification type lookup and category/priority validation map to existing generic codes — no new error codes are registered. This follows the precedent set by the segments/storyboards/subtitles domains.

## Implementation Notes

### Source Module Layout

Per the project's domain five-file pattern ([PROJECT_STRUCTURE.md](PROJECT_STRUCTURE.md)):

```
server/src/domains/notifications/
├── mod.rs         # Module declarations + router assembly
├── error.rs       # NotificationsError enum (6 variants)
├── types.rs       # Row / Request / Response DTOs (three-type pattern)
├── service.rs     # Service layer (SQL queries, BOLA-scoped by user_id)
└── handlers.rs    # Thin HTTP translation (extractors → service → Json<T>)
```

### BOLA Prevention

Every mutation (`mark_read`, `mark_all_read`, `delete`, `delete_all_read`) binds `user_id` directly into the SQL `WHERE` clause. A user cannot affect another user's notifications even with a valid notification UUID. If `rows_affected() == 0`, the service returns `NotificationsError::NotFound` — indistinguishable from "doesn't exist" vs "belongs to another user" (no information leakage). This matches the bookmark delete pattern from the playback domain.

### Cursor Pagination Reuse

The cursor encode/decode helpers mirror the media domain (`server/src/domains/media/service.rs:526-538`):
- `parse_cursor(cursor) -> Option<Uuid>` — base64 decode → JSON parse → extract "id" → parse UUID
- `encode_cursor(id) -> String` — JSON `{"id": "..."}` → base64 encode

UUIDv7's embedded timestamp means `id` ordering == chronological ordering. No separate `created_at` cursor is needed.

### Mark-All-Read Implementation

`POST /api/v1/notifications/read-all` runs a single bulk UPDATE:

```sql
UPDATE notifications SET is_read = true, read_at = now()
WHERE user_id = $1 AND is_read = false
```

Returns the count of rows affected. No cursor pagination on this endpoint — it's a single bulk operation. The partial index `idx_notifications_unread` makes this efficient.

### Notification Preferences DTO

The preferences list endpoint returns all notification types with the user's per-type overrides (or system defaults when no explicit row exists). Most users will have zero explicit preference rows — they accept defaults from `notification_types.is_enabled_by_default`. The endpoint materializes defaults alongside explicit overrides so the client doesn't need a second round-trip.

```json
{
  "preferences": [
    {
      "notification_type_id": "...",
      "name": "new_media_added",
      "category": "media",
      "priority": "low",
      "in_app_enabled": true,      // explicit override OR default
      "webhook_enabled": false,    // explicit override OR default
      "push_enabled": false,       // explicit override OR default
      "is_using_defaults": true    // false when explicit row exists
    }
  ]
}
```

### Test Notification Endpoint (Admin)

`POST /api/v1/notifications/test` accepts an optional notification type (default `server_alert`) and dispatches to the calling admin via the existing `services::notification_dispatch::dispatch()` pipeline. This serves the Phase 13b verification flow ("Admin triggers a test notification. Notification appears in-app, via SSE, and via webhook"). The test endpoint:

- Requires `Require<CanManageServer>` (admin-only)
- Accepts `{ "notification_type": "server_alert", "title": "Optional override", "body": "Optional override" }`
- Dispatches via the standard pipeline (DB-write-first + SSE + webhook + mobile push when configured)
- Returns the `DispatchResult` so the admin can verify per-channel status

### Migration Notifications

Phase 14 Task 13 adds the `migration_completed` and `migration_failed` notification types. They are produced by `workers::migration_runner` for users who can manage migrations and use the standard dispatch pipeline, so they appear in the notification center, the navbar bell SSE feed, and any enabled webhook channel without migration-specific client code.

### Not Implemented (Deferred to Future Tasks)

- **`user_push_devices` table + registration API** — ✅ Implemented in Phase 13b Task 5. See [MOBILE_PUSH.md](MOBILE_PUSH.md) "Phase 13b Task 5 implementation notes". Four routes under `/api/v1/user/push-devices` (register + list + heartbeat + revoke) with token lifecycle (24h heartbeat → 30-day stale deactivation).
- **Notifications UI** (notification center, preferences editor, push device management) — ✅ Implemented in Phase 13b Task 6. See BUILD_ORDER.md Phase 13b Task 6 notes. Navbar bell + dropdown for quick access; `/settings/notifications` full page with 3 tabs (Feed / Preferences / Push Devices) + admin-only test dispatch section.
- **Email delivery channel** — future; `email_template` column exists in `notification_types` but no SMTP infrastructure
- **Scheduled digests** (daily/weekly email summary) — future enhancement
