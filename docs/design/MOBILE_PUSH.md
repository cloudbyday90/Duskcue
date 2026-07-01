# Mobile Push Gateway

## Overview

This document is the authoritative design for delivering Duskcue notifications to mobile devices (iOS, Android) when the mobile app is backgrounded or offline. Server-Sent Events ([REAL_TIME_PUSH.md](REAL_TIME_PUSH.md)) handle push while the app is open; this document handles the case where the app is closed or the OS has suspended it — which requires OS-level push notification infrastructure (FCM for Android, APNs for iOS).

The decision documented here: **Mobile push is opt-in and admin-configured, never on by default.** The default notification channels are in-app + SSE + webhook. Mobile push requires the operator to provide credentials. **Webhook is the recommended primary "push" channel for self-hosted deployments** because it works without Google/Apple intermediaries — operators forward to ntfy, Gotify, Discord, Telegram, or any HTTP target. **FCM is the recommended mobile-native push** when admin provides Firebase credentials (covers Android + iOS via one integration). **APNs direct** is the privacy-preserving iOS-only alternative. **UnifiedPush** is documented for Android-only privacy-maximalist deployments.

## Scope

**Covers:**

- Mobile push channel selection (FCM vs APNs vs UnifiedPush vs webhook-only)
- Default-on vs opt-in posture (security/privacy tradeoffs)
- Per-user device registration and token lifecycle
- Per-notification-type opt-in (user controls which notifications push)
- Payload structure, size limits, end-to-end encryption
- Operator configuration (Firebase service account JSON, APNs .p8 key, UnifiedPush endpoint, webhook URL)
- Notification dispatch architecture (Phase 13 forcing function)
- Flutter client integration (Phase 16)
- Privacy implications of routing through Google/Apple

**Does NOT cover:**

- In-app notifications (always on; REST-queryable)
- SSE event push for foreground web/desktop clients — see [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md)
- Notification template localization — see [I18N.md](I18N.md)
- Email delivery (future; Phase 13)
- Webhook payload schema (covered briefly here; full schema in Phase 13 implementation)

## Decision — Multi-Channel Dispatch with Webhook as Default

Duskcue's notification system dispatches each notification to N configurable channels. Mobile push is one channel category; it is **opt-in and admin-configured**, never on by default. The four channel categories:

| Channel | Default | When Used | Setup Required |
|---|---|---|---|
| **In-app** | ✅ Always on | Notification center; REST-queryable | None |
| **SSE** | ✅ Always on | Foreground web/desktop/Tauri client | None (uses existing session auth) |
| **Webhook** | ✅ Recommended default for "push" | Forward to ntfy/Gotify/Discord/Slack/Telegram/any HTTP target | Admin configures webhook URL + optional secret |
| **Mobile push** | ❌ Opt-in | Backgrounded iOS/Android app | Admin provides FCM service account JSON OR APNs .p8 key OR UnifiedPush endpoint |

### Why Webhook Is the Recommended Default for "Push"

For self-hosted media server deployments, **webhook is the lowest-friction push mechanism that preserves Duskcue's local-first values**:

1. **No Google/Apple intermediary** — Operator chooses the destination: ntfy (self-hosted), Gotify (self-hosted), Discord, Slack, Telegram, Pushover, a custom script. Duskcue just POSTs JSON to a URL.
2. **Cross-platform via the destination** — ntfy has Android + iOS + web + CLI clients; Gotify has Android + web; Discord/Slack/Telegram have native apps everywhere. One webhook config reaches all the user's devices.
3. **No SDK dependency** — Duskcue just makes an HTTP POST with `reqwest` (already in workspace). No FCM SDK, no APNs SDK, no Firebase project.
4. **Operator controls reliability** — Self-hosted ntfy on the same LAN as Duskcue = no internet dependency for push. Compare to FCM (Google reliability) or APNs (Apple reliability).
5. **Privacy-preserving** — Notification content stays within the operator's infrastructure when forwarded to a self-hosted ntfy/Gotify instance.

**The trade-off:** Webhook doesn't deliver directly to the mobile lock screen. It delivers to the relay (ntfy/Gotify/Discord/Telegram), which then handles the OS-level push via its own infrastructure (ntfy uses FCM for the public server, or foreground-service instant-delivery for F-Droid/self-hosted; Discord uses its own FCM/APNs integration; etc.). For most users this indirection is fine — they already have ntfy/Discord/Telegram installed.

### Why Mobile Push Is Opt-In (Not Default)

Mobile-native push (FCM, APNs) is opt-in because:

1. **Privacy values** — Duskcue is local-first and security-conscious (per [SECURITY.md](../security/SECURITY.md)). Routing notification content through Google (FCM) or Apple (APNs) is a values tension. Users who want maximum privacy use the webhook channel with a self-hosted ntfy instead.
2. **Setup complexity** — FCM requires creating a Firebase project; APNs requires an Apple Developer account ($99/year) and generating a .p8 key. Most operators don't want to do this for a personal media server.
3. **No middleman for LAN-only deployments** — The default Duskcue deployment is LAN-only (local mode per [SECURITY.md](../security/SECURITY.md) tier 1). Mobile push via FCM/APNs requires internet egress, which local-mode users may not have or want.
4. **Notification volume** — Media servers generate low-priority notifications ("new media added", "library scan complete"). These don't warrant OS-level push by default; in-app + SSE is sufficient for the common case. Users who want push for high-priority notifications (e.g., trust alerts, failed backups) opt in.

### Mobile Push Channel Selection (When Opted In)

When an admin enables mobile push, they choose the channel:

| Channel | Android | iOS | Google Middleman | Apple Middleman | Setup Complexity | Recommended For |
|---|---|---|---|---|---|---|
| **FCM (Firebase)** | ✅ Direct | ✅ Via APNs gateway | ✅ Required | ✅ Required (FCM→APNs) | Medium (Firebase project) | Operators who want one integration for both platforms; don't mind Google routing |
| **APNs direct** | ❌ | ✅ Direct | ❌ None | ✅ Required | Medium (Apple Developer account) | iOS-only deployments that want to avoid Google; Apple-only is unavoidable on iOS |
| **UnifiedPush** | ✅ Via distributor (ntfy, FCM-free) | ❌ Not supported | ❌ None | ❌ None | High (self-hosted ntfy + UnifiedPush-aware Android app) | Android-only privacy-maximalist deployments (LineageOS, F-Droid users) |
| **Webhook** | ✅ Via ntfy/Gotify/Discord | ✅ Via ntfy/Discord/Telegram | Depends on relay | Depends on relay | Low (one URL) | All deployments; recommended default |

**Hard constraint:** iOS has no self-hosted push option. Apple requires all iOS push to route through APNs. There is no UnifiedPush-for-iOS; no ntfy-direct-to-iOS-lock-screen. Operators who want iOS push must accept Apple infrastructure. This is an Apple platform limitation, not a Duskcue design choice.

**Recommended path for most operators:** Webhook → ntfy (self-hosted). Covers Android (ntfy F-Droid app with instant delivery), iOS (ntfy iOS app), web (ntfy PWA), and CLI. No Firebase project, no Apple Developer account, no Google routing for Android.

## Notification Dispatch Architecture

This section defines the Phase 13 notification dispatch architecture. Phase 13 must implement this; if it ships without it, adding mobile push later requires reworking the dispatch layer.

### Dispatch Pipeline

```
┌──────────────────────────────────────────────────────────────────────┐
│                     Notification Dispatch                             │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  1. EVENT SOURCE                                                      │
│     Any domain that wants to notify a user:                           │
│     - Library scan complete (scan worker)                             │
│     - New media added (scan worker)                                   │
│     - Scheduled task failed (scheduler)                               │
│     - Trust alert (analytics/security worker)                         │
│     - Backup failed (backup worker)                                   │
│     - Admin invitation (auth domain)                                  │
│                                                                       │
│  2. NOTIFICATION RECORD (always)                                      │
│     - INSERT INTO notifications (user_id, type, title, body, etc.)   │
│     - Always happens; in-app channel is always on                     │
│     - Locale-aware title/body via Fluent (see I18N.md)                │
│                                                                       │
│  3. CHANNEL FAN-OUT (async, per-user preferences)                     │
│     ┌─────────────┬─────────────┬─────────────┬─────────────┐        │
│     │ SSE         │ Webhook     │ Mobile push │ Email       │        │
│     │ (if open)   │ (if config) │ (if config) │ (future)    │        │
│     └─────────────┴─────────────┴─────────────┴─────────────┘        │
│                                                                       │
│  4. PER-CHANNEL DELIVERY                                              │
│     - SSE: publish to EventBus (DashMap per user)                    │
│     - Webhook: HTTP POST to operator-configured URL                  │
│     - Mobile push: FCM / APNs / UnifiedPush per device token         │
│     - Email: SMTP (future)                                           │
│                                                                       │
└──────────────────────────────────────────────────────────────────────┘
```

**Critical design rule:** The notification record (step 2) ALWAYS exists in the database, regardless of which channels deliver it. This means:

- Users can always see missed notifications via REST (`GET /api/v1/notifications`) even if no push channel was configured
- The in-app notification center is the source of truth; channels are delivery optimizations
- If a channel fails (FCM 5xx, webhook timeout, APNs cert expired), the notification is still visible in-app — no data loss

### Per-User Channel Preferences

Each user has per-notification-type channel preferences in `user_notification_preferences` (table created in Phase 2). Schema extension for push channels:

```sql
ALTER TABLE user_notification_preferences
ADD COLUMN push_enabled BOOLEAN NOT NULL DEFAULT false,
ADD COLUMN webhook_enabled BOOLEAN NOT NULL DEFAULT true;
```

Defaults: `push_enabled = false`, `webhook_enabled = true`. Users opt into push per notification type via the user settings UI (Phase 13).

### Per-User Device Registration

For mobile push (FCM/APNs/UnifiedPush), each user registers their device(s):

```sql
CREATE TABLE user_push_devices (
    id UUID DEFAULT uuidv7() PRIMARY KEY,
    created_at TIMESTAMPTZ GENERATED ALWAYS AS (uuid_extract_timestamp(id)) STORED,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),

    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Push provider for this device
    provider TEXT NOT NULL CHECK (provider IN ('fcm', 'apns', 'unifiedpush')),
    
    -- Provider-specific token/endpoint
    -- FCM: registration token (string, ~152 chars)
    -- APNs: device token (hex string, 64 chars)
    -- UnifiedPush: endpoint URL (string, URL)
    token TEXT NOT NULL,
    
    -- Device metadata for display in user settings
    device_name TEXT,  -- "Alice's iPhone 15", "Pixel 8 Pro"
    platform TEXT,     -- "ios", "android"
    app_version TEXT,  -- "1.0.0"
    
    -- Lifecycle
    last_seen_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT true,
    
    -- FCM tokens can be revoked by Google; APNs tokens can change on restore
    invalidated_at TIMESTAMPTZ,
    
    UNIQUE(user_id, provider, token)
);

CREATE INDEX idx_user_push_devices_user ON user_push_devices (user_id) WHERE is_active = true;
```

**Token lifecycle:**
- **Registration** — Mobile app calls `POST /api/v1/user/push-devices` with `{provider, token, device_name, platform, app_version}`. Server validates the token by sending a test push.
- **Heartbeat** — Mobile app calls `PUT /api/v1/user/push-devices/{id}` every 24h with `{last_seen_at: now()}`. Devices not seen in 30 days are marked inactive.
- **Invalidation** — If FCM/APNs returns "token revoked" or "device unregistered," the server sets `invalidated_at` and `is_active = false`. The mobile app re-registers on next launch.
- **Manual revoke** — User can delete a device via `DELETE /api/v1/user/push-devices/{id}`.

## Provider Implementations

### Webhook (Always Available)

The simplest channel. Operator configures:

```json
{
  "webhook": {
    "url": "https://ntfy.example.com/duskcue",
    "secret": "shared-secret-for-hmac-signing",
    "format": "ntfy"  // "ntfy" | "gotify" | "discord" | "slack" | "generic"
  }
}
```

Duskcue POSTs to the URL with a format-specific payload:

- **`ntfy` format** — `POST https://ntfy.example.com/duskcue` with `Title:` and `Priority:` headers, body as plaintext. Matches ntfy's [publish API](https://ntfy.sh/docs/publish/).
- **`gotify` format** — `POST https://gotify.example.com/message?token=...` with JSON body `{title, message, priority}`.
- **`discord` format** — `POST` to Discord webhook URL with `{content: "..."}` or richer embed.
- **`slack` format** — `POST` to Slack incoming webhook with `{text: "..."}`.
- **`generic` format** — `POST` with full notification JSON + `X-Duskcue-Signature` HMAC-SHA256 header.

**HMAC signing** — When `secret` is configured, all webhook deliveries include `X-Duskcue-Signature: sha256=<hex>` computed over the request body. Recipients verify to prevent forgery.

**Retry policy** — Webhook failures retry with exponential backoff (1s, 5s, 30s, 2m, 10m). After 5 failures, the webhook is marked degraded and the admin is notified (in-app). No data loss — notification is in the DB.

### FCM (Firebase Cloud Messaging)

**Setup:** Admin provides Firebase service account JSON (downloaded from Firebase Console → Project Settings → Service Accounts → Generate New Private Key). Stored encrypted at rest per the existing encryption service (Phase 6 Task 13).

**Rust implementation:** Direct HTTP v1 API (no Rust Firebase Admin SDK exists). Duskcue:

1. Parses service account JSON (`project_id`, `private_key`, `client_email`)
2. Generates a JWT from the private key with the `firebase.messaging` scope
3. Exchanges JWT for OAuth 2.0 access token at `https://oauth2.googleapis.com/token`
4. POSTs to `https://fcm.googleapis.com/v1/projects/{project_id}/messages:send` with `Authorization: Bearer {access_token}`
5. Access tokens expire in 1 hour; Duskcue caches and refreshes automatically

**Payload:**

```json
{
  "message": {
    "token": "<device-registration-token>",
    "notification": {
      "title": "New media added",
      "body": "The Matrix was added to Movies"
    },
    "data": {
      "notification_id": "01950abc-...",
      "type": "new_media_added",
      "media_item_id": "01950def-...",
      "action_url": "/media/01950def-..."
    },
    "android": {
      "priority": "high",
      "notification": {
        "icon": "@ic_notification",
        "color": "#c8965a"
      }
    },
    "apns": {
      "payload": {
        "aps": {
          "sound": "default",
          "badge": 1
        }
      }
    }
  }
}
```

**FCM covers both Android and iOS** — the Flutter app uses `firebase_messaging` which handles platform-specific delivery. Duskcue's server sends one FCM message; Google routes to Android (via FCM) or iOS (via APNs gateway).

### APNs Direct

**Setup:** Admin provides Apple Developer Team ID + Key ID + `.p8` private key (generated at Apple Developer → Certificates, Identifiers & Profiles → Keys → Apple Push Notifications key). Stored encrypted at rest.

**Rust implementation:** Uses the [`a2`](https://crates.io/crates/a2) crate (pure Rust APNs client). Token-based auth (JWT from `.p8` key); no certificate renewal needed (keys don't expire).

**Payload (alert):**

```json
{
  "aps": {
    "alert": {
      "title": "New media added",
      "body": "The Matrix was added to Movies"
    },
    "sound": "default",
    "badge": 1
    "mutable-content": 1
  },
  "notification_id": "01950abc-...",
  "type": "new_media_added",
  "media_item_id": "01950def-...",
  "action_url": "/media/01950def-..."
}
```

**APNs is iOS-only.** Android devices registered with APNs provider are ignored. Admins who choose APNs direct must also configure FCM or UnifiedPush for Android users.

### UnifiedPush (Android-Only, Privacy-Maximalist)

**Setup:** No server-side credentials needed. The mobile app registers with a UnifiedPush distributor (typically ntfy) and reports the resulting endpoint URL to Duskcue.

**Implementation:** Duskcue treats UnifiedPush as a special webhook — the device's `token` is the endpoint URL returned by the distributor. Duskcue POSTs to that URL; the distributor forwards to the device.

**Payload:** Same as `generic` webhook format — JSON body with notification fields.

**Limitation:** iOS has no UnifiedPush support. This channel is Android-only. iOS users on the same Duskcue instance need FCM or APNs for push.

## Privacy Analysis

### Data Exposure by Channel

| Channel | Notification content visible to | Encryption |
|---|---|---|
| **In-app** | Server operator only (stored in PG) | At-rest encryption via DB; TLS in transit |
| **SSE** | Server operator; client browser | TLS in transit |
| **Webhook** | Operator + relay operator (ntfy/Gotify/Discord) | TLS in transit; HMAC signing for integrity |
| **FCM** | Operator + Google | TLS in transit to Google; Google has access to notification payload |
| **APNs** | Operator + Apple | TLS in transit to Apple; Apple has access to notification payload |
| **UnifiedPush** | Operator + distributor operator (self-hosted ntfy = operator) | TLS in transit; distributor sees payload |

**Recommendation for privacy-sensitive deployments:** Use webhook → self-hosted ntfy. Notification content stays entirely within the operator's infrastructure. No Google, no Apple, no third party.

### Payload Minimization

Regardless of channel, Duskcue minimizes the notification payload:

- **Title + body** are always present (localized via Fluent per [I18N.md](I18N.md))
- **Data fields** (`notification_id`, `type`, `media_item_id`, `action_url`) use UUIDs and relative URLs — no media metadata, no user PII, no library contents
- **No media content** — Notification doesn't include poster art, overview, or cast; the mobile app fetches those via REST when the user opens the notification
- **No session tokens** — Notifications don't include auth tokens; the mobile app uses its own session

This ensures that even if Google/Apple/the relay inspects the payload, they learn only "a notification was delivered" — not what media it's about, who the user is, or how to access the library.

## Flutter Client Integration (Phase 16)

The Flutter mobile app (Phase 16a) integrates mobile push as part of the native client foundation defined in [DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md). Provider choices remain opt-in and admin-configured, but the mobile app must be able to register and refresh provider tokens when the selected provider is available.

Preferred client integrations:

```yaml
# pubspec.yaml
dependencies:
  # FCM (recommended mobile-native push when opted in)
  firebase_messaging: latest-compatible

  # APNs direct (alternative for iOS-only, no Google)
  # Use platform channels or a maintained APNs plugin if direct APNs registration is required

  # UnifiedPush (alternative for Android, no Google)
  # Use a maintained UnifiedPush plugin or platform channel; Android-only
```

The exact package versions are selected during Phase 16a Task 1 so they match the generated Flutter SDK baseline. Task 0 intentionally avoids pinning stale versions before the project is generated.

**App-side push registration flow:**

1. App launches → requests OS push permission (iOS) / checks FCM consent (Android)
2. Obtains push token (FCM token, APNs device token, or UnifiedPush endpoint)
3. Calls `POST /api/v1/user/push-devices` with `{provider, token, device_name, platform, app_version}`
4. Server sends a silent test push to validate the token → marks device active
5. App receives pushes → displays notification → on tap, deep-links to the relevant screen

**Background handling:** The Flutter app registers a background isolate (via `firebase_messaging.onBackgroundMessage`) to process incoming data payloads. For visible notifications, the OS displays the alert; the app doesn't need to be running.

## Edge Cases

### Token Invalidation Mid-Flight

FCM tokens can be revoked by Google (app uninstall, app data clear, Google Play Services update). APNs tokens can change after device restore. Duskcue handles "token revoked" responses by:

1. Marking the device `is_active = false`, `invalidated_at = now()`
2. Retrying the notification via fallback channels (webhook, in-app)
3. On next app launch, the mobile app re-registers with a new token

### Rate Limiting by Provider

- **FCM** — No documented rate limit for individual sends; high-volume topic sends are rate-limited
- **APNs** — Rate limit is per-device; Duskcue never sends >1 notification/second to a single device
- **UnifiedPush** — Depends on the distributor; self-hosted ntfy has no rate limit

Duskcue's rate limiter (existing `RateLimitState`) extends to cover outbound push: max 10 notifications/minute per user across all push channels. Prevents a runaway worker from spamming the user's phone.

### Offline Server (No Internet Egress)

If Duskcue can't reach FCM/APNs (e.g., LAN-only deployment, internet outage):

1. Push delivery fails gracefully — notification is still in-app and via SSE
2. Failed pushes are logged but don't block the dispatch pipeline
3. When internet returns, missed HIGH-priority notifications can be re-pushed (configurable: "retry push on reconnect" — default off to avoid notification storms)

### Multiple Devices per User

A user may have multiple devices (phone + tablet, Android + iOS). Each device registers independently. Duskcue fans out push to ALL active devices for the user. If a user has 3 devices and one token is invalid, the other two still receive the push.

### Do Not Disturb (DND) / Focus Modes

Duskcue respects OS-level DND via the `priority` field in push payloads:

- **Low priority** (`new_media_added`, `library_scan_complete`) — respects DND; silent on iOS when DND is on
- **High priority** (`trust_alert`, `backup_failed`, `task_failed`) — bypasses DND on Android; respects DND on iOS (Apple doesn't allow third-party apps to bypass DND without a special entitlement)

### Notification Grouping

If a user receives 10 "new media added" notifications in quick succession (bulk library scan), the mobile push channel groups them:

- **iOS** — APNs supports `thread-id` for grouping; Duskcue sets `thread-id: "new_media_added"` so iOS collapses them into one stack
- **Android** — FCM supports `notification.android.tag` for grouping; Duskcue sets the tag so Android replaces rather than stacks

### Webhook Recipient Down

If the webhook recipient (ntfy, Discord, etc.) is down:

1. Duskcue retries with exponential backoff (1s, 5s, 30s, 2m, 10m)
2. After 5 failures, webhook is marked degraded; admin notified in-app
3. No data loss — notifications are in the DB; when webhook recipient returns, admin can trigger re-delivery (future enhancement)

### Operator Changes Push Provider

If an admin switches from FCM to APNs (or vice versa):

1. Existing device tokens are provider-specific and invalid for the new provider
2. Duskcue marks all existing push devices as `is_active = false` on provider config change
3. Mobile apps re-register on next launch with tokens for the new provider
4. Transition window: push doesn't work until apps re-register (typically < 24 hours)

## Implementation Status

| Component | Status | Notes |
|---|---|---|
| `notifications` table + `notification_types` seed | ✅ Implemented | Phase 2 migration |
| Fluent notification template rendering | ✅ Implemented | Phase 13b Task 1 — `services/i18n.rs`; `notification_types.in_app_template` migrated from English strings to Fluent message IDs |
| Multi-channel dispatch pipeline | ✅ Implemented | Phase 13b Task 2 + Phase 16a Task 9 — `services/notification_dispatch.rs`; DB-write-first + SSE fan-out + webhook + mobile push fan-out |
| Offline download ready/failed notifications | ✅ Implemented | Phase 16c Task 8 — `download_ready` and `download_failed` notification types are seeded; ready/final-failed download jobs dispatch through the same in-app/SSE/webhook/opt-in mobile push pipeline, while noisy progress remains foreground SSE only |
| In-app notification center (REST API) | ✅ Implemented | Phase 13b Task 3 — `domains/notifications/` (10 routes: list + unread-count + mark-read single/all + delete single/all + notification-types + preferences list/update + admin test dispatch). See [NOTIFICATIONS.md](NOTIFICATIONS.md) |
| SSE event bus for `notification` events | ✅ Wired by dispatch pipeline | `services/event_bus.rs` (Phase 10 Task 11); dispatch pipeline calls `state.event_bus.publish(user_id, ServerEvent::new("notification", payload))` |
| Webhook dispatch (basic: generic format + HMAC signing) | ✅ Implemented | Phase 13b Task 2 — `notification_dispatch::dispatch_webhook()`; generic JSON payload + `X-Duskcue-Signature` HMAC-SHA256 |
| Webhook dispatch (formats: ntfy/Gotify/Discord/Slack + retry) | ✅ Implemented | Phase 13b Task 4 — `WebhookFormat` enum + `format_request()` + exponential-backoff retry with full jitter (1s, 5s, 30s, 2m, 10m); retryable-status classification; `Retry-After` honored. See "Phase 13b Task 4 implementation notes" below |
| `user_push_devices` table + registration API | ✅ Implemented | Phase 13b Task 5 — `domains/notifications/` (4 routes: register + list + heartbeat + revoke). Token lifecycle (24h heartbeat → 30-day stale deactivation wired into `notification_cleanup` worker). See "Phase 13b Task 5 implementation notes" below |
| Push dispatch fan-out | ✅ Implemented | Phase 16a Task 9 — asynchronous FCM/APNs/UnifiedPush delivery with terminal delivery-status updates |
| Notification delivery Prometheus metrics | ✅ Implemented | Pre-v1.0 Task 4 — `notification_delivery_total{channel,status}` for in-app, SSE, webhook, and push; webhook records both scheduled `pending` and terminal `delivered`/`failed` statuses |
| FCM HTTP v1 client (Rust) | ✅ Implemented | Uses service-account JWT OAuth and `projects/{project_id}/messages:send` |
| APNs token-auth client | ✅ Implemented | Uses ES256 provider token auth and APNs HTTP/2 request headers |
| UnifiedPush endpoint delivery | ✅ Implemented | Treats device token as distributor endpoint URL and POSTs minimized JSON |
| Per-user channel preferences UI | ✅ Implemented | Phase 13b Task 6 — `/settings/notifications` Preferences tab: per-notification-type × per-channel (in-app/webhook/push) toggle matrix with dirty-state per-row save; available to all authenticated users (per-user preferences, not admin-gated) |
| Notifications UI | ✅ Implemented | Phase 13b Task 6 — `NotificationBell.svelte` (navbar bell + unread badge + dropdown panel with recent notifications); `notificationCenter.js` store (SSE `notification` event subscription + 60s polling fallback + optimistic mutations); `/settings/notifications` page with Feed/Preferences/Push-Devices tabs + admin-only test dispatch |
| Flutter `firebase_messaging` integration | ✅ Implemented | Registers FCM tokens, APNs tokens on iOS, token refresh, background/tap handlers |
| Flutter UnifiedPush integration | Partial | Optional Android platform channel `duskcue/mobile_push.getUnifiedPushEndpoint`; native distributor adapter still requires device-side implementation |

**Phase 13b Task 2 implementation notes:**

- **Dispatch pipeline module**: `services/notification_dispatch.rs` — cross-cutting service consumed by workers (library scan, backup, analytics) and HTTP handlers. Follows the `services/` convention (like `event_bus.rs`, `encryption.rs`).
- **DB-write-first guarantee**: The notification record is INSERT-ed to `notifications` before any channel fan-out. If all channels fail, the notification is still visible in-app (the in-app channel IS the DB record — it's always on by design).
- **SSE fan-out is synchronous**: `EventBus::publish()` is a fast in-memory broadcast (no I/O). The dispatch pipeline calls it directly rather than `tokio::spawn`-ing it, since it's sub-microsecond.
- **Webhook fan-out is fire-and-forget via `tokio::spawn`**: The HTTP POST to the webhook URL runs in a background task. Failures are logged at WARN and recorded in `notifications.delivery_status` JSONB. The dispatch pipeline does not await webhook completion — the caller (e.g., a library scan worker) should not block on webhook delivery latency.
- **Push fan-out is active**: The dispatch pipeline resolves push config + preferences, returns `push: "pending"` immediately, and runs provider delivery in a background task. The task updates `delivery_status.push` to `delivered`, `failed`, or `skipped` after the provider batch completes.
- **Per-user locale rendering**: The dispatch pipeline reads the user's preferred locale from `users.metadata->>'locale'` (server-side dispatch has no HTTP Accept-Language header). Falls back to the base English locale. All notification title/body text is rendered via `services::i18n::render()` before DB INSERT, so the stored notification is already localized.
- **Webhook HMAC signing**: `X-Duskcue-Signature: sha256=<hex>` header computed via `ring::hmac` over the raw request body bytes. This follows the GitHub `X-Hub-Signature-256` / Hook0 `X-Hook0-Signature` convention (the de-facto standard for webhook integrity verification as of 2025-2026). Task 4 will reuse this signing mechanism for all webhook formats.
- **`NotificationConfig` expansion**: The empty placeholder `NotificationConfig` in `state.rs` is expanded with `webhook` (url, secret, format) and `push` (enabled, provider) sub-configs. The webhook secret is encrypted at rest via the existing `EncryptionKey` (AES-256-GCM), matching the metadata/subtitle/Trakt provider key pattern.
- **`user_notification_preferences.push_enabled`**: Migration adds the `push_enabled BOOLEAN NOT NULL DEFAULT false` column per the design doc's schema extension. The existing `webhook_enabled` column (Phase 2, default false) is reused as-is; users opt in per notification type.
- **Idempotency**: The notification UUID (UUIDv7) is included in the webhook payload as `notification_id` so recipients can deduplicate. This follows the webhook best practice identified in research (Hook0 docs, June 2026).
- **Channel preference resolution**: When no `user_notification_preferences` row exists for a user + notification type, the dispatch pipeline uses sensible defaults: `in_app_enabled = true`, `webhook_enabled = false`, `push_enabled = false`. The `notification_types.is_enabled_by_default` flag gates whether the notification type is active at all.
- **Delivery metrics**: Pre-v1.0 Task 4 records `notification_delivery_total{channel,status}` after each channel status update. Webhook and push both record the initial scheduled state as `pending`, then their background tasks record terminal `delivered`, `failed`, or `skipped` outcomes after provider work completes.

**Phase 13b Task 2 key decisions:**

1. **Webhook secret encrypted at rest** — Uses the existing `EncryptionKey` + `decrypt_notification_config()` helper (same pattern as metadata/subtitle/Trakt provider keys). The decrypted secret is in the live `RuntimeConfig` for dispatch use; never logged.
2. **Webhook fire-and-forget over synchronous** — Workers calling `dispatch()` should not block on webhook HTTP latency (potentially seconds for external services). The spawned task handles the POST and records `delivery_status`; the caller gets an immediate `DispatchResult` with `webhook: "pending"`.
3. **Generic webhook format in Task 2, rich formats in Task 4** — Task 2 ships a `generic` JSON payload with HMAC signing (sufficient for operators using ntfy/generic endpoints). Task 4 adds format-specific payloads (ntfy headers, Gotify/Discord/Slack JSON shapes) and retry with exponential backoff.
4. **Push dispatch is fire-and-forget** — The fan-out logic checks config + preferences and schedules provider work after the DB insert. Provider failures never block in-app/SSE delivery, and terminal push status is written back to `notifications.delivery_status`.
5. **`ring::hmac` for webhook signing** — `ring` is already in the workspace (rustls, PBKDF2, AES-256-GCM). No new HMAC crate needed. `ring::hmac::{Key, HMAC_SHA256, sign}` is the canonical API.

**Phase 13b Task 3 implementation notes:**

- **In-app notification center is the in-app channel** — The dispatch pipeline (Task 2) writes the notification record to the DB; Task 3 exposes that record via REST. The DB record IS the in-app channel — "the notification record always exists in the database, regardless of which channels deliver it" (Task 2 key decision). Task 3 makes that guarantee visible to clients.
- **Notification CRUD surface** — 10 routes under `/api/v1/notifications` + `/api/v1/notification-types` + `/api/v1/user/notification-preferences`. All user-scoped routes require `AuthenticatedUser` (BOLA enforced at SQL layer — `user_id` bound into every `WHERE` clause). The single admin route (`POST /api/v1/notifications/test`) requires `Require<CanManageServer>` and dispatches via the existing pipeline.
- **Cursor pagination per API_CONVENTIONS.md** — Notifications use cursor pagination ("Chronological feed" per the Pagination Strategy table). `notifications.id` is `UUID DEFAULT uuidv7()` — UUIDv7's embedded timestamp makes the primary key naturally time-ordered. Cursor encode/decode reuses the media domain's base64+JSON pattern.
- **Preferences materialize defaults** — `GET /api/v1/user/notification-preferences` LEFT JOINs `notification_types` with `user_notification_preferences`. Most users will have zero explicit preference rows — they accept defaults from `notification_types.is_enabled_by_default`. The `is_using_defaults: bool` flag tells the UI whether the user has explicitly overridden anything for that type.
- **Test endpoint for verification** — `POST /api/v1/notifications/test` (admin-only) dispatches via the existing pipeline. Serves the Phase 13b verification criterion: "Admin triggers a test notification. Notification appears in-app, via SSE, and via webhook." Returns per-channel status (`in_app`/`sse`/`webhook`/`push`) so the admin can verify fan-out.
- **See [NOTIFICATIONS.md](NOTIFICATIONS.md)** for the authoritative design — route table, authorization model, pagination strategy, error handling, and implementation notes for the in-app notification center API.

**Phase 13b Task 4 implementation notes:**

- **Five payload formats via `WebhookFormat` enum** — `Generic`, `Ntfy`, `Gotify`, `Discord`, `Slack`. `WebhookFormat::from_config()` parses `server_config.notifications.webhook.format`; **unknown values fall back to `Generic`** so a typo never breaks dispatch (infallible by design — does not implement `FromStr`, which would require returning `Result`). `format_request()` produces a `FormattedRequest { url, content_type, headers, body }` consumed by the retry loop. Each format renders from the already-localized `title`/`body` strings — no per-format i18n.
- **`generic`** — unchanged from Task 2: full Duskcue notification JSON + `X-Duskcue-Signature` HMAC-SHA256 over the body when a secret is configured. The default; the only format that carries `notification_id`/`metadata`/`related_item_*` fields.
- **`ntfy`** — plain-text body (`"{title}\n\n{body}"`) with `Title:`, `Priority:`, `Tags:`, `Markdown: yes` headers per the [ntfy publish API](https://docs.ntfy.sh/publish/). Priority maps Duskcue `low`/`medium`/`high` → ntfy 1-5 scale (2/3/5). Tags are emoji shortcuts per category (`security` → `rotating_light,warning`, `media` → `film_projector`, etc.). Content-Type is `text/plain`.
- **`gotify`** — JSON `{title, message, priority}` body. Priority maps to Gotify's 0-10 scale (2/5/8). The Gotify app **token stays in the operator-configured URL** (`?token=...`); Duskcue adds no auth header (Gotify accepts query-string or `X-Gotify-Key` header auth). Per [Gotify push docs](https://gotify.net/docs/pushmsg).
- **`discord`** — JSON `{username: "Duskcue", content: "**{title}**\n{body}"}`. **Content is truncated to 2000 chars** (Discord's hard cap; truncation is by Unicode scalar value via `.chars().take(2000)`, not bytes). Appends `?wait=true` (or `&wait=true` if the URL already has a query) so Discord returns a real status code instead of the default 204 fire-and-forget reply. Per [Discord webhook docs](https://docs.discord.com/developers/resources/webhook).
- **`slack`** — JSON `{text: "*{title}*\n{body}"}`. Slack mrkdwn is on by default for incoming webhooks; `*...*` = bold. Per [Slack incoming webhooks](https://api.slack.com/incoming-webhooks).
- **HMAC signing applies to ALL formats** — `sign_request()` appends `X-Duskcue-Signature: sha256=<hex>` over the (format-specific) body bytes when a secret is configured, regardless of format. This is the GitHub `X-Hub-Signature-256` / Hook0 convention reused from Task 2. For ntfy/Gotify the operator's URL-token is the primary auth; the HMAC secret is optional defense-in-depth.
- **Retry with exponential backoff + full jitter** — `dispatch_webhook()` attempts delivery once immediately, then retries up to `WEBHOOK_BACKOFF_SECONDS.len()` times with the schedule `[1s, 5s, 30s, 2m, 10m]` applied before each retry (per MOBILE_PUSH.md §Retry policy). **Full jitter** (`jittered_duration()`: 0.5×–1.5× the base) is applied to every wait to prevent thundering-herd spikes when many notifications fail simultaneously, per Hookdeck/Svix retry best-practice guides (June 2026).
- **Retryable vs non-retryable classification** — `is_retryable_status()` returns true for `408 | 429 | 500 | 502 | 503 | 504` (transient). All other non-2xx (including `400`/`401`/`403`/`404`/`405`/`410`/`422`) are treated as permanent and abort the retry loop immediately. This is critical for Discord: a `404` means the webhook was deleted, and repeated retries would count toward Discord's 10,000-invalid-requests-per-10-minutes IP ban threshold.
- **`Retry-After` honored on 429** — `send_once()` parses the `Retry-After` header (integer-seconds form, used by all four providers) and `dispatch_webhook()` waits that long (capped at 10 minutes so a misconfigured endpoint can't stall delivery indefinitely) before the next attempt. HTTP-date form is ignored (returns `None`; rare for these services).
- **`WebhookError` restructured into 4 variants** — `ClientBuild` (terminal; config/transport issue), `RequestFailed` (network/timeout — retryable), `NonRetryableStatus { status, body }` (4xx — abort), `RetryableStatus { status, retry_after, body }` (5xx/429/408 — back off and retry). The old single `NonSuccessStatus` variant is split so the retry loop can branch cleanly.
- **Fire-and-forget preserved** — `spawn_webhook_delivery()` still spawns via `tokio::spawn`; the caller (worker/handler) gets an immediate `DispatchResult` with `webhook: "pending"`. The retry loop runs entirely within the spawned task. After exhaustion, `delivery_status.webhook` is set to `"failed"`. The notification record in the DB is always the source of truth — webhook failure is never data loss.
- **No "degraded + admin notified" yet** — MOBILE_PUSH.md §Retry policy specifies "After 5 failures, the webhook is marked degraded and the admin is notified (in-app)." The degraded-state tracking and admin notification (a self-referential notification via the dispatch pipeline) is **deferred** to avoid recursion risk; current behavior marks `delivery_status.webhook = "failed"` and logs WARN with all retry attempts. The admin sees the failed status in the notification center.
- **Admin UI gains `webhook_format` selector** — `/settings/system` Notifications group now exposes a dropdown of `generic`/`ntfy`/`gotify`/`discord`/`slack`. Existing `webhook_url`/`webhook_secret` hints updated to clarify token placement (URL for ntfy/gotify/discord/slack) and that the secret is optional HMAC for all formats.
- **No new workspace dependencies** — all functionality uses existing `reqwest` 0.12 (already configured for `rustls-tls`, `no_proxy`, `redirect::none` per API_SECURITY.md SSRF hardening), `ring::hmac`, `rand` 0.9 (jitter), `tokio::time::sleep` (backoff). The webhook client builder adds `connect_timeout(10s)` alongside the existing `timeout(15s)`.
- **16 new unit tests + 2 `tokio::test` integration tests** (27 total in `services::notification_dispatch`): format parsing (case-insensitive + unknown fallback), all 5 format bodies (generic JSON / ntfy plain-text+headers / gotify JSON+priority / discord truncation + `?wait=true` query handling / slack mrkdwn), HMAC header presence/absence on sign, retryable-status classification (6 retryable + 7 permanent), `Retry-After` parsing (integer OK / HTTP-date + garbage rejected), jitter band `[0.5×, 1.5×)`, backoff schedule equals doc values, plus two `tokio::test` integration tests with a raw `TcpListener` HTTP mock verifying `send_once` classifies a real 429+Retry-After as `RetryableStatus` and a real 404 as `NonRetryableStatus`. All 638 server tests pass (620 prior + 18 new in dispatch module). 0 clippy warnings, 0 svelte-check warnings.

**Phase 13b Task 5 implementation notes:**

- **Schema matches this doc's `user_push_devices` DDL** — Migration `20260629010000_create_user_push_devices.sql` creates the table with `UNIQUE(user_id, provider, token)` for upsert-safe re-registration and a partial index `WHERE is_active = true` for efficient active-device lookups by the dispatch pipeline. The `provider` CHECK constraint matches `PushDispatchConfig::is_configured()`'s `matches!("fcm"|"apns"|"unifiedpush")`.
- **Four routes under `/api/v1/user/push-devices`** — `POST` (register/upsert), `GET` (list the user's devices), `PUT /{device_id}` (heartbeat + optional metadata refresh), `DELETE /{device_id}` (manual revoke = hard DELETE). All user-scoped via `AuthenticatedUser` with BOLA enforced at the SQL layer (`user_id` bound into every `WHERE` clause) — same pattern as bookmarks and sessions.
- **No pattern validation for FCM/APNs tokens** — Research (June 2026) confirmed both Google ([FCM manage-tokens docs](https://firebase.google.com/docs/cloud-messaging/manage-tokens)) and Apple ([APNs send docs](https://developer.apple.com/documentation/usernotifications/sending-notification-requests-to-apns): *"Don't make assumptions about device token size"*) explicitly warn that token formats may change and should not be validated against patterns. Duskcue validates only: non-empty + ≤4096 chars + printable ASCII (bytes ≥ 0x20). Strict validation now happens at delivery time (provider returns `UNREGISTERED`/`BadDeviceToken`/`Unregistered`/404/410 → server-side invalidation).
- **UnifiedPush tokens ARE URL-validated** — Unlike FCM/APNs, UnifiedPush tokens are endpoint URLs returned by the distributor (e.g., `https://ntfy.example.com/duskcue-up/abcdef`). `url::Url::parse` validation catches malformed registrations early. Matches this doc's "Duskcue treats UnifiedPush as a special webhook" design.
- **Registration is an upsert (idempotent re-registration)** — `ON CONFLICT (user_id, provider, token) DO UPDATE SET last_seen_at = now(), is_active = true, invalidated_at = NULL`. This reactivates previously invalidated devices (e.g., app re-install with the same FCM token after Google rotation) and refreshes metadata via COALESCE. Mobile apps should call `POST` on every launch — no separate "register vs heartbeat" decision for the client.
- **Heartbeat (`PUT /{device_id}`) requires `is_active = true`** — The `WHERE id = $1 AND user_id = $2 AND is_active = true` clause means heartbeats on invalidated devices return `PushDeviceNotFound` (BOLA-safe: no information leakage about whether the device exists but is inactive vs. doesn't exist at all). The mobile app re-registers via `POST` on next launch, which reactivates.
- **30-day stale-device deactivation wired into `notification_cleanup`** — The existing scheduled task (every 1h, per Phase 2 seed) now calls `deactivate_stale_devices()` after deleting expired notifications. Devices with `last_seen_at < now() - INTERVAL '30 days'` are set `is_active = false, invalidated_at = now()`. Configurable via task config `stale_device_days` (default 30, clamped to [1, 3650]). This implements the "Devices not seen in 30 days are marked inactive" rule without a new scheduled task.
- **Provider-revoked token invalidation implemented in Phase 16a Task 9** — FCM `UNREGISTERED`, APNs `BadDeviceToken`/`Unregistered`, and UnifiedPush 404/410 responses set `is_active = false`, `invalidated_at = now()`, and refresh `updated_at`. Logs include provider/reason only, never the token.
- **Token preview (masked) in responses** — `PushDeviceResponse.token_preview` shows first 8 + last 4 chars with `…` separator (e.g., `c2aK9KHm…9b`). Tokens shorter than 12 chars show `***`. This minimizes token exposure in API responses, browser devtools, and logs while still letting users identify which device is which alongside the `device_name` field.
- **Manual revoke is a hard DELETE** — `DELETE /{device_id}` removes the row entirely (not a soft-delete). Rationale: the user explicitly wants the device gone; re-registration creates a new row; soft-deleted rows would accumulate as dead weight. This differs from *automatic* invalidation (provider response or staleness) which uses `is_active = false` + `invalidated_at` so the dispatch pipeline can skip the device and the mobile app can detect the invalidation and re-register.
- **Three new `NotificationsError` variants** — `PushDeviceNotFound` (SYS_004, 404 — reuses the existing notification-not-found code; BOLA-safe), `InvalidPushProvider` (VALID_001, 422), `InvalidPushToken` (VALID_001, 422). No new error codes registered per the established precedent. Mapped in `notifications_error_to_http()` alongside the existing 6 variants.
- **11 new unit tests** covering: provider validation (accept known/reject unknown), token validation (reject empty/whitespace, reject overlong, accept FCM token, accept APNs hex, reject non-ASCII, accept UnifiedPush URL, reject UnifiedPush non-URL), token masking (short → `***`, long → prefix…suffix), optional-length validation (reject overlong, accept None + in-bounds). All 651 server tests pass (638 prior + 13 new across notifications service). 0 clippy warnings.
- **No new workspace dependencies** — All validation uses existing `url` crate (Phase 4 Task 2 for WebAuthn); all DB access uses existing `sqlx`; all routing/validation uses existing `axum` + `validator`.

**Phase 16a Task 11 settings integration notes:**

- The Flutter settings hub consumes `GET /api/v1/user/notification-preferences`, `PUT /api/v1/user/notification-preferences/{type_id}`, `GET /api/v1/user/push-devices`, and `DELETE /api/v1/user/push-devices/{device_id}` so users can edit in-app/push/webhook delivery choices and revoke stale mobile push devices without opening the web UI.
- Provider credential setup and admin test-dispatch workflows remain web-first in `/settings/notifications` because they are operator/server configuration, not per-user mobile account preferences.

## Key Decisions

1. **Mobile push is opt-in, never default** — Routing notification content through Google (FCM) or Apple (APNs) is a values tension for a local-first, security-conscious media server. Users who want push opt in; default is in-app + SSE + webhook.
2. **Webhook is the recommended "push" channel** — Works without Google/Apple intermediaries; operator chooses the destination (ntfy self-hosted, Discord, Telegram, etc.); one config reaches all user devices via the relay's own push infrastructure; uses existing `reqwest` (no new SDK dependency).
3. **FCM is the recommended mobile-native push when opted in** — One integration covers Android + iOS via `firebase_messaging` Flutter plugin. Admin provides Firebase service account JSON; Duskcue uses direct HTTP v1 API (no Rust Firebase Admin SDK exists, but HTTP v1 is straightforward).
4. **APNs direct for iOS-only privacy-preserving deployments** — Uses `a2` crate; token-based auth (.p8 key, no cert renewal); avoids Google middleman for iOS. Android users on the same instance need FCM or UnifiedPush separately.
5. **UnifiedPush for Android-only privacy-maximalist deployments** — No Google dependency; uses UnifiedPush distributor (typically ntfy). iOS has no UnifiedPush equivalent (Apple platform limitation). Documented but not the primary recommendation.
6. **Multi-channel dispatch from day one** — Phase 13 notification system fans out to in-app + SSE + webhook + mobile-push simultaneously. Notification record always exists in DB regardless of channel delivery; no data loss if a channel fails. Adding channels later (email, SMS, Matrix) plugs into the existing fan-out.
7. **Per-user device registration with token lifecycle** — `user_push_devices` table tracks active tokens per provider; heartbeat every 24h; auto-invalidation on provider "token revoked" response; manual revoke via user settings.
8. **Payload minimization** — Notifications carry title + body + UUID-based data fields only. No media content, no session tokens, no PII. Even if Google/Apple inspects the payload, they learn nothing about the user's library.
9. **iOS push requires Apple infrastructure — no self-hosted alternative** — Hard platform constraint. Operators who want iOS push must accept APNs. Documented clearly to set expectations.
10. **Rate limiting on outbound push** — Max 10 notifications/minute per user across all push channels. Prevents runaway workers from spamming phones. Existing `RateLimitState` extends to cover outbound.
11. **DND respected per priority** — Low-priority notifications respect OS DND; high-priority (`trust_alert`, `backup_failed`) bypass DND on Android (iOS doesn't allow third-party DND bypass without special entitlement).
12. **Notification grouping for bulk events** — Library scans producing 50 "new media added" notifications collapse to one stack via `thread-id` (iOS) / `tag` (Android).

## Relationship to Other Domains

| Document | Relationship |
|---|---|
| [REAL_TIME_PUSH.md](REAL_TIME_PUSH.md) | SSE handles foreground push; this document handles background push. Together they cover the full push lifecycle. |
| [I18N.md](I18N.md) | Notification title/body localized via Fluent per recipient's locale. Push payload carries the localized strings, not translation keys. |
| [SECURITY.md](../security/SECURITY.md) | Three-tier network model; mobile push requires internet egress (tier 3 only); webhook works on tier 1 (LAN) if relay is on LAN. |
| [MULTI_INSTANCE.md](MULTI_INSTANCE.md) | Single-instance assumption; push state (`user_push_devices`, dispatch queue) is PG-backed. EventBus for SSE is in-memory (acceptable per MULTI_INSTANCE.md). |
| [API_CONVENTIONS.md](API_CONVENTIONS.md) | `POST /api/v1/user/push-devices` device registration endpoint; standard session auth. |
| [DESKTOP_MOBILE_CLIENTS.md](DESKTOP_MOBILE_CLIENTS.md) | Phase 16a desktop/mobile client platform decisions, including foreground SSE, mobile-native push, and secure token storage. |
| [BUILD_ORDER.md](../../BUILD_ORDER.md) | Phase 13 (notification system) is the forcing function. Phase 16a (Flutter mobile) integrates `firebase_messaging` or equivalent provider clients plus UnifiedPush registration. |

## Research Sources

- **[Firebase Cloud Messaging HTTP v1 API](https://firebase.google.com/docs/cloud-messaging/send/v1-api)** — Official FCM docs; OAuth 2.0 service account auth; message payload structure
- **[FCM legacy API deprecation](https://stackoverflow.com/questions/78144233/firebase-cloud-messaging-api-http-v1-migration)** — Legacy FCM API fully deprecated June 2024; HTTP v1 or Admin SDK required
- **[ntfy self-hosted push notification server](https://ntfy.sh/)** — Open source, self-hostable push via HTTP pub/sub; Android + iOS + web + CLI clients; UnifiedPush distributor
- **[UnifiedPush](https://unifiedpush.org/)** — Open standard for push without Google/Apple; Android-only (iOS has no alternative to APNs); distributor apps (ntfy, FCM-fallback)
- **[ntfy as UnifiedPush distributor](https://unifiedpush.org/users/distributors/ntfy/)** — How ntfy forwards UnifiedPush messages; F-Droid build has no Firebase
- **[Self-hosted Mobile Push Notifications using NTFY](https://thejeshgn.com/2022/08/23/self-hosted-mobile-push-notifications-using-ntfy/)** — Practical deployment guide; LineageOS / F-Droid use case; instant delivery via foreground service
- **[`a2` crate (Rust APNs client)](https://crates.io/crates/a2)** — Pure-Rust APNs client; token-based auth; async
- **[`firebase-messaging` Rust crate](https://crates.io/crates/firebase-messaging)** — Community Rust client for FCM HTTP v1 (not official Google SDK)
- **[Apple Push Notification service](https://developer.apple.com/documentation/usernotifications)** — APNs docs; token-based auth with .p8 keys; payload limits (4KB standard, 5KB VoIP)
- **[Flutter `firebase_messaging` plugin](https://pub.dev/packages/firebase_messaging)** — Official Flutter FCM integration; Android + iOS via one plugin
- **[Gotify self-hosted push](https://gotify.net/)** — Alternative self-hosted push server; Android + web clients; mTLS support
- **[ntfy publish API](https://docs.ntfy.sh/publish/)** — ntfy message format: plain-text body with `Title`/`Priority`/`Tags`/`Markdown` headers; Priority 1-5 scale
- **[Gotify push message API](https://gotify.net/docs/pushmsg)** — Gotify `{title, message, priority}` JSON; app token via query string or `X-Gotify-Key` header; priority 0-10
- **[Discord Execute Webhook](https://docs.discord.com/developers/resources/webhook)** — `{content, username, embeds}`; content 2000-char cap; `?wait=true` for real status; 404 = deleted webhook (don't retry)
- **[Discord Rate Limits](https://docs.discord.com/developers/topics/rate-limits)** — 429 + `Retry-After` semantics; invalid-request IP ban at 10k/10min (401/403/429)
- **[Slack incoming webhooks](https://api.slack.com/incoming-webhooks)** — `{text}` payload; mrkdwn default-on; no per-webhook retry (rate limited globally)
- **[Webhook Retry Best Practices (Hookdeck)](https://hookdeck.com/outpost/guides/outbound-webhook-retry-best-practices)** — Retryable (408/429/5xx) vs non-retryable (4xx) classification; full jitter recommendation; honor `Retry-After`
- **[Webhook Retry Strategies (Svix)](https://www.svix.com/resources/webhook-university/reliability/webhook-retry-strategies/)** — Exponential backoff + full jitter; 6-8 attempts over 24-48h as industry default; stop retrying permanent (4xx) failures
