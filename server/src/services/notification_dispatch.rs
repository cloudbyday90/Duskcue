// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Multi-channel notification dispatch pipeline.
//!
//! Implements the dispatch architecture from
//! [MOBILE_PUSH.md](../../docs/design/MOBILE_PUSH.md) §Notification Dispatch
//! Architecture and [BUILD_ORDER.md](../../BUILD_ORDER.md) Phase 13b Task 2:
//!
//! 1. **DB-write-first (always)** — The notification record is INSERT-ed to the
//!    `notifications` table before any channel fan-out. If all channels fail,
//!    the notification is still visible in-app.
//! 2. **SSE fan-out (always, synchronous)** — Publishes a `notification` event
//!    to the [`EventBus`](crate::services::event_bus::EventBus) for live
//!    foreground clients. This is sub-microsecond (in-memory broadcast).
//! 3. **Webhook fan-out (fire-and-forget)** — Spawns a background task to POST
//!    to the operator-configured webhook URL with HMAC-SHA256 signing. Task 4
//!    adds format-specific payloads (ntfy/Gotify/Discord/Slack) and retry.
//! 4. **Push fan-out (stub)** — Resolves push config + preferences but the
//!    actual FCM/APNs/UnifiedPush client is deferred to Phase 16a.
//!
//! ## Locale rendering
//!
//! The dispatch pipeline renders the notification title/body via Fluent
//! ([`crate::services::i18n`]) before DB INSERT, using the recipient's preferred
//! locale from `users.metadata->>'locale'`. Server-side dispatch has no HTTP
//! `Accept-Language` header; the user preference is the sole locale source.
//!
//! ## Usage
//!
//! Workers and handlers call [`dispatch`] to send a notification:
//!
//! ```ignore
//! use crate::services::notification_dispatch::{dispatch, NotificationInput};
//!
//! dispatch(&state, NotificationInput::new(
//!     user_id,
//!     "new_media_added",
//!     serde_json::json!({"title": "The Matrix", "library": "Movies"}),
//! )).await?;
//! ```

use std::collections::HashMap;

use ring::hmac::{Key, HMAC_SHA256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Row};
use unic_langid::LanguageIdentifier;
use uuid::Uuid;

use crate::services::event_bus::ServerEvent;
use crate::services::i18n;
use crate::state::AppState;

const FETCH_NOTIFICATION_TYPE_SQL: &str = r#"
    SELECT id, category, priority, in_app_template, is_enabled_by_default
    FROM notification_types
    WHERE name = $1
"#;

const FETCH_USER_LOCALE_SQL: &str = r#"
    SELECT metadata->>'locale' AS locale, display_name
    FROM users
    WHERE id = $1 AND deleted_at IS NULL
"#;

const FETCH_USER_PREFS_SQL: &str = r#"
    SELECT in_app_enabled, webhook_enabled, push_enabled
    FROM user_notification_preferences
    WHERE user_id = $1 AND notification_type_id = $2
"#;

const INSERT_NOTIFICATION_SQL: &str = r#"
    INSERT INTO notifications (
        user_id, notification_type_id, title, body, priority, link,
        delivery_channels, delivery_status, related_item_type, related_item_id, metadata
    )
    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
    RETURNING id
"#;

const UPDATE_DELIVERY_STATUS_SQL: &str = r#"
    UPDATE notifications SET delivery_status = $2 WHERE id = $1
"#;

/// Input for dispatching a notification.
///
/// Callers provide the notification type name (looked up in
/// `notification_types`), metadata for Fluent template rendering, and optional
/// overrides. When `title`/`body` are `None`, the dispatch pipeline renders
/// them from the Fluent template (`notification_types.in_app_template` is the
/// Fluent message ID).
#[derive(Debug, Clone)]
pub struct NotificationInput {
    pub user_id: Uuid,
    pub notification_type: String,
    pub metadata: Value,
    pub title: Option<String>,
    pub body: Option<String>,
    pub link: Option<String>,
    pub related_item_type: Option<String>,
    pub related_item_id: Option<Uuid>,
}

impl NotificationInput {
    pub fn new(user_id: Uuid, notification_type: &str, metadata: Value) -> Self {
        Self {
            user_id,
            notification_type: notification_type.to_string(),
            metadata,
            title: None,
            body: None,
            link: None,
            related_item_type: None,
            related_item_id: None,
        }
    }
}

/// Status of a single delivery channel after dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Delivered,
    Pending,
    Skipped,
    Failed,
    NotImplemented,
}

impl ChannelStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ChannelStatus::Delivered => "delivered",
            ChannelStatus::Pending => "pending",
            ChannelStatus::Skipped => "skipped",
            ChannelStatus::Failed => "failed",
            ChannelStatus::NotImplemented => "not_implemented",
        }
    }
}

/// Result of a dispatch operation — the notification ID and per-channel status.
#[derive(Debug, Clone, Serialize)]
pub struct DispatchResult {
    pub notification_id: Uuid,
    pub in_app: ChannelStatus,
    pub sse: ChannelStatus,
    pub webhook: ChannelStatus,
    pub push: ChannelStatus,
}

/// Dispatch error — returned when the dispatch pipeline itself fails (not when
/// a delivery channel fails — those are recorded in `DispatchResult`).
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("Notification type '{0}' not found")]
    NotificationTypeNotFound(String),
    #[error("Notification type '{0}' is disabled")]
    NotificationTypeDisabled(String),
    #[error("User '{0}' not found")]
    UserNotFound(Uuid),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Dispatch a notification to all applicable channels.
///
/// This is the primary entry point for the notification system. Workers and
/// handlers call this function to send a notification to a user. The function:
///
/// 1. Looks up the notification type (must exist and be enabled)
/// 2. Resolves the user's preferred locale
/// 3. Renders title/body via Fluent (unless overridden in the input)
/// 4. INSERTs the notification record to the DB (in-app channel)
/// 5. Publishes a `notification` SSE event via the EventBus
/// 6. Spawns a background task for webhook delivery (if configured + enabled)
/// 7. Resolves push config (stub — actual push deferred to Phase 16a)
/// 8. Returns a [`DispatchResult`] with per-channel status
pub async fn dispatch(state: &AppState, input: &NotificationInput) -> Result<DispatchResult, DispatchError> {
    let pool = &state.pool;

    let nt = fetch_notification_type(pool, &input.notification_type).await?;
    if !nt.is_enabled_by_default {
        tracing::debug!(
            notification_type = %input.notification_type,
            "Notification type is disabled; skipping dispatch"
        );
        return Err(DispatchError::NotificationTypeDisabled(
            input.notification_type.clone(),
        ));
    }

    let user_info = fetch_user_info(pool, input.user_id).await?;

    let locale = i18n::negotiate_locale(user_info.locale.as_deref(), None);
    let (rendered_title, rendered_body) =
        render_notification(&nt.in_app_template, &locale, &input.metadata, &input.title, &input.body);

    let prefs = fetch_user_prefs(pool, input.user_id, nt.id).await;

    let delivery_channels = build_delivery_channels(&prefs, state);
    let delivery_status = serde_json::json!({
        "in_app": "pending",
        "sse": "pending",
        "webhook": "pending",
        "push": "pending"
    });

    let notification_id: Uuid = sqlx::query_scalar(INSERT_NOTIFICATION_SQL)
        .bind(input.user_id)
        .bind(nt.id)
        .bind(&rendered_title)
        .bind(&rendered_body)
        .bind(&nt.priority)
        .bind(&input.link)
        .bind(&delivery_channels)
        .bind(&delivery_status)
        .bind(&input.related_item_type)
        .bind(input.related_item_id)
        .bind(&input.metadata)
        .fetch_one(pool)
        .await?;

    let sse_status = publish_sse(state, input.user_id, notification_id, &nt, &rendered_title, &rendered_body, input);

    let in_app_status = ChannelStatus::Delivered;

    let push_status = dispatch_push(state, input.user_id, &nt, &prefs);

    let webhook_status = if prefs.webhook_enabled && state.runtime_config.load().notifications.webhook.is_configured() {
        let webhook_config = state.runtime_config.load();
        let webhook = webhook_config.notifications.webhook.clone();
        drop(webhook_config);
        spawn_webhook_delivery(
            pool.clone(),
            notification_id,
            webhook,
            &nt,
            &rendered_title,
            &rendered_body,
            input,
        );
        ChannelStatus::Pending
    } else {
        ChannelStatus::Skipped
    };

    let final_status = serde_json::json!({
        "in_app": in_app_status.as_str(),
        "sse": sse_status.as_str(),
        "webhook": webhook_status.as_str(),
        "push": push_status.as_str()
    });
    let _ = sqlx::query(UPDATE_DELIVERY_STATUS_SQL)
        .bind(notification_id)
        .bind(&final_status)
        .execute(pool)
        .await;

    Ok(DispatchResult {
        notification_id,
        in_app: in_app_status,
        sse: sse_status,
        webhook: webhook_status,
        push: push_status,
    })
}

fn build_delivery_channels(prefs: &UserPrefs, state: &AppState) -> Value {
    let config = state.runtime_config.load();
    let mut channels = vec!["in_app"];

    if prefs.webhook_enabled && config.notifications.webhook.is_configured() {
        channels.push("webhook");
    }

    if prefs.push_enabled && config.notifications.push.is_configured() {
        channels.push("push");
    }

    channels.push("sse");
    serde_json::to_value(channels).unwrap_or_else(|_| serde_json::json!(["in_app"]))
}

fn render_notification(
    template_id: &str,
    locale: &LanguageIdentifier,
    metadata: &Value,
    title_override: &Option<String>,
    body_override: &Option<String>,
) -> (String, String) {
    let title = title_override.clone().unwrap_or_else(|| {
        let title_key = format!("{template_id}-title");
        let rendered = i18n::render(&title_key, locale, &HashMap::new());
        if rendered == title_key {
            "Duskcue".to_string()
        } else {
            rendered
        }
    });

    let body = body_override.clone().unwrap_or_else(|| {
        let args = match metadata {
            Value::Object(map) => i18n::args_from_metadata(map),
            _ => HashMap::new(),
        };
        i18n::render(template_id, locale, &args)
    });

    (title, body)
}

fn publish_sse(
    state: &AppState,
    user_id: Uuid,
    notification_id: Uuid,
    nt: &NotificationTypeInfo,
    title: &str,
    body: &str,
    input: &NotificationInput,
) -> ChannelStatus {
    let payload = serde_json::json!({
        "id": notification_id,
        "notification_type": input.notification_type,
        "category": nt.category,
        "priority": nt.priority,
        "title": title,
        "body": body,
        "link": input.link,
        "related_item_type": input.related_item_type,
        "related_item_id": input.related_item_id,
        "created_at": chrono::Utc::now(),
    });

    state.event_bus.publish(user_id, ServerEvent::new("notification", payload));
    ChannelStatus::Delivered
}

fn dispatch_push(state: &AppState, _user_id: Uuid, nt: &NotificationTypeInfo, prefs: &UserPrefs) -> ChannelStatus {
    let config = state.runtime_config.load();
    if !prefs.push_enabled {
        return ChannelStatus::Skipped;
    }
    if !config.notifications.push.is_configured() {
        return ChannelStatus::Skipped;
    }

    tracing::info!(
        notification_type = %nt.category,
        "Push dispatch: provider configured but FCM/APNs/UnifiedPush client not yet implemented (Phase 16a)"
    );
    ChannelStatus::NotImplemented
}

fn spawn_webhook_delivery(
    pool: PgPool,
    notification_id: Uuid,
    config: crate::state::WebhookDispatchConfig,
    nt: &NotificationTypeInfo,
    title: &str,
    body: &str,
    input: &NotificationInput,
) {
    let payload = build_webhook_payload(notification_id, nt, title, body, input);
    let body_bytes = match serde_json::to_vec(&payload) {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!(notification_id = %notification_id, error = %e, "Failed to serialize webhook payload");
            return;
        }
    };

    let url = match &config.url {
        Some(u) => u.clone(),
        None => return,
    };
    let secret = config.secret.clone();
    let format = config.format.clone();

    tokio::spawn(async move {
        let result = dispatch_webhook(&url, &secret, &format, &body_bytes).await;
        let status_str = match &result {
            Ok(()) => "delivered",
            Err(e) => {
                tracing::warn!(
                    notification_id = %notification_id,
                    url = %url,
                    error = %e,
                    "Webhook delivery failed"
                );
                "failed"
            }
        };

        let current_status: Value = match sqlx::query("SELECT delivery_status FROM notifications WHERE id = $1")
            .bind(notification_id)
            .fetch_one(&pool)
            .await
        {
            Ok(row) => row.try_get("delivery_status").unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };

        let mut status_map = match current_status {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        status_map.insert("webhook".to_string(), Value::String(status_str.to_string()));

        let _ = sqlx::query("UPDATE notifications SET delivery_status = $2 WHERE id = $1")
            .bind(notification_id)
            .bind(Value::Object(status_map))
            .execute(&pool)
            .await;
    });
}

fn build_webhook_payload(
    notification_id: Uuid,
    nt: &NotificationTypeInfo,
    title: &str,
    body: &str,
    input: &NotificationInput,
) -> Value {
    serde_json::json!({
        "notification_id": notification_id,
        "type": input.notification_type,
        "category": nt.category,
        "priority": nt.priority,
        "title": title,
        "body": body,
        "link": input.link,
        "related_item_type": input.related_item_type,
        "related_item_id": input.related_item_id,
        "metadata": input.metadata,
        "created_at": chrono::Utc::now().to_rfc3339(),
    })
}

async fn dispatch_webhook(
    url: &str,
    secret: &Option<String>,
    _format: &str,
    body_bytes: &[u8],
) -> Result<(), WebhookError> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| WebhookError::ClientBuild(e.to_string()))?;

    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Duskcue-Notifications/1.0");

    if let Some(secret) = secret
        && !secret.is_empty()
    {
        let signature = compute_hmac_signature(secret.as_bytes(), body_bytes);
        request = request.header("X-Duskcue-Signature", format!("sha256={signature}"));
    }

    let response = request
        .body(body_bytes.to_vec())
        .send()
        .await
        .map_err(|e| WebhookError::RequestFailed(e.to_string()))?;

    let status = response.status();
    if status.is_success() {
        Ok(())
    } else {
        let body_text = response.text().await.unwrap_or_default();
        Err(WebhookError::NonSuccessStatus {
            status: status.as_u16(),
            body: body_text,
        })
    }
}

fn compute_hmac_signature(secret: &[u8], body: &[u8]) -> String {
    let key = Key::new(HMAC_SHA256, secret);
    let tag = ring::hmac::sign(&key, body);
    hex_encode(tag.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[derive(Debug, thiserror::Error)]
enum WebhookError {
    #[error("Failed to build HTTP client: {0}")]
    ClientBuild(String),
    #[error("Request failed: {0}")]
    RequestFailed(String),
    #[error("Webhook returned non-success status {status}: {body}")]
    NonSuccessStatus { status: u16, body: String },
}

struct NotificationTypeInfo {
    id: Uuid,
    category: String,
    priority: String,
    in_app_template: String,
    is_enabled_by_default: bool,
}

async fn fetch_notification_type(
    pool: &PgPool,
    name: &str,
) -> Result<NotificationTypeInfo, DispatchError> {
    let row = sqlx::query(FETCH_NOTIFICATION_TYPE_SQL)
        .bind(name)
        .fetch_optional(pool)
        .await?;

    let row = row.ok_or_else(|| DispatchError::NotificationTypeNotFound(name.to_string()))?;

    Ok(NotificationTypeInfo {
        id: row.try_get("id").unwrap_or_else(|_| Uuid::nil()),
        category: row.try_get("category").unwrap_or_else(|_| "system".to_string()),
        priority: row.try_get("priority").unwrap_or_else(|_| "low".to_string()),
        in_app_template: row
            .try_get("in_app_template")
            .unwrap_or_else(|_| name.to_string()),
        is_enabled_by_default: row.try_get("is_enabled_by_default").unwrap_or(true),
    })
}

struct UserInfo {
    locale: Option<String>,
    #[allow(dead_code)]
    display_name: Option<String>,
}

async fn fetch_user_info(pool: &PgPool, user_id: Uuid) -> Result<UserInfo, DispatchError> {
    let row = sqlx::query(FETCH_USER_LOCALE_SQL)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    let row = row.ok_or(DispatchError::UserNotFound(user_id))?;

    Ok(UserInfo {
        locale: row.try_get("locale").unwrap_or(None),
        display_name: row.try_get("display_name").unwrap_or(None),
    })
}

struct UserPrefs {
    #[allow(dead_code)]
    in_app_enabled: bool,
    webhook_enabled: bool,
    push_enabled: bool,
}

impl Default for UserPrefs {
    fn default() -> Self {
        Self {
            in_app_enabled: true,
            webhook_enabled: false,
            push_enabled: false,
        }
    }
}

async fn fetch_user_prefs(pool: &PgPool, user_id: Uuid, notification_type_id: Uuid) -> UserPrefs {
    let result = sqlx::query(FETCH_USER_PREFS_SQL)
        .bind(user_id)
        .bind(notification_type_id)
        .fetch_optional(pool)
        .await;

    match result {
        Ok(Some(row)) => UserPrefs {
            in_app_enabled: row.try_get("in_app_enabled").unwrap_or(true),
            webhook_enabled: row.try_get("webhook_enabled").unwrap_or(false),
            push_enabled: row.try_get("push_enabled").unwrap_or(false),
        },
        Ok(None) => UserPrefs::default(),
        Err(e) => {
            tracing::warn!(
                user_id = %user_id,
                error = %e,
                "Failed to fetch user notification preferences; using defaults"
            );
            UserPrefs::default()
        }
    }
}

/// Dispatch a notification to multiple users (broadcast).
///
/// Convenience wrapper that calls [`dispatch`] for each user independently.
/// Per-user failures (user not found, disabled type) are logged and skipped;
/// the function returns only the successful dispatch results.
pub async fn dispatch_to_many(
    state: &AppState,
    user_ids: &[Uuid],
    notification_type: &str,
    metadata: Value,
) -> Vec<DispatchResult> {
    let mut results = Vec::with_capacity(user_ids.len());
    for &user_id in user_ids {
        let input = NotificationInput::new(user_id, notification_type, metadata.clone());
        match dispatch(state, &input).await {
            Ok(result) => results.push(result),
            Err(e) => {
                tracing::warn!(
                    user_id = %user_id,
                    notification_type = %notification_type,
                    error = %e,
                    "Failed to dispatch notification to user"
                );
            }
        }
    }
    results
}

/// Dispatch a notification to all users who have access to a library.
///
/// Queries `user_library_access` for the library's user IDs, then dispatches
/// to each. Returns the count of successful deliveries.
pub async fn dispatch_to_library_members(
    state: &AppState,
    library_id: Uuid,
    notification_type: &str,
    metadata: Value,
) -> Result<usize, DispatchError> {
    let user_ids: Vec<Uuid> = sqlx::query_scalar(
        r#"
        SELECT user_id FROM user_library_access
        WHERE library_id = $1
        UNION
        SELECT id FROM users
        WHERE role = 'owner' AND deleted_at IS NULL
        "#,
    )
    .bind(library_id)
    .fetch_all(&state.pool)
    .await?;

    let results = dispatch_to_many(state, &user_ids, notification_type, metadata).await;
    Ok(results.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_encode_produces_lowercase_hex() {
        assert_eq!(hex_encode(&[0x00, 0xff, 0xab]), "00ffab");
        assert_eq!(hex_encode(&[]), "");
        assert_eq!(hex_encode(&[0x01]), "01");
    }

    #[test]
    fn hmac_signature_is_deterministic() {
        let secret = b"my-secret-key";
        let body = b"{\"test\":true}";
        let sig1 = compute_hmac_signature(secret, body);
        let sig2 = compute_hmac_signature(secret, body);
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64);
    }

    #[test]
    fn hmac_signature_changes_with_different_body() {
        let secret = b"key";
        let sig1 = compute_hmac_signature(secret, b"body1");
        let sig2 = compute_hmac_signature(secret, b"body2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn channel_status_as_str_is_correct() {
        assert_eq!(ChannelStatus::Delivered.as_str(), "delivered");
        assert_eq!(ChannelStatus::Pending.as_str(), "pending");
        assert_eq!(ChannelStatus::Skipped.as_str(), "skipped");
        assert_eq!(ChannelStatus::Failed.as_str(), "failed");
        assert_eq!(ChannelStatus::NotImplemented.as_str(), "not_implemented");
    }

    #[test]
    fn notification_input_new_sets_fields() {
        let input = NotificationInput::new(
            Uuid::nil(),
            "new_media_added",
            serde_json::json!({"title": "Movie"}),
        );
        assert_eq!(input.notification_type, "new_media_added");
        assert!(input.title.is_none());
        assert!(input.body.is_none());
        assert!(input.link.is_none());
    }

    #[test]
    fn webhook_payload_contains_required_fields() {
        let nt = NotificationTypeInfo {
            id: Uuid::nil(),
            category: "media".to_string(),
            priority: "low".to_string(),
            in_app_template: "new-media-added".to_string(),
            is_enabled_by_default: true,
        };
        let input = NotificationInput::new(
            Uuid::nil(),
            "new_media_added",
            serde_json::json!({"title": "Matrix"}),
        );
        let payload = build_webhook_payload(Uuid::nil(), &nt, "Title", "Body", &input);

        assert_eq!(payload["type"], "new_media_added");
        assert_eq!(payload["category"], "media");
        assert_eq!(payload["priority"], "low");
        assert_eq!(payload["title"], "Title");
        assert_eq!(payload["body"], "Body");
        assert!(payload["notification_id"].is_string());
        assert!(payload["created_at"].is_string());
    }

    #[test]
    fn default_user_prefs_matches_design() {
        let prefs = UserPrefs::default();
        assert!(prefs.in_app_enabled);
        assert!(!prefs.webhook_enabled);
        assert!(!prefs.push_enabled);
    }

    #[test]
    fn render_notification_uses_overrides_when_provided() {
        let locale = i18n::DEFAULT_LOCALE;
        let metadata = serde_json::json!({"key": "value"});
        let title_override = Some("Custom Title".to_string());
        let body_override = Some("Custom Body".to_string());

        let (title, body) = render_notification(
            "new-media-added",
            &locale,
            &metadata,
            &title_override,
            &body_override,
        );

        assert_eq!(title, "Custom Title");
        assert_eq!(body, "Custom Body");
    }

    #[test]
    fn render_notification_falls_back_to_fluent() {
        let locale = i18n::DEFAULT_LOCALE;
        let metadata = serde_json::json!({"title": "Inception", "library": "Movies"});
        let none_title = None;
        let none_body = None;

        let (_title, body) = render_notification(
            "new-media-added",
            &locale,
            &metadata,
            &none_title,
            &none_body,
        );

        assert_eq!(body, "Inception was added to Movies");
    }
}
