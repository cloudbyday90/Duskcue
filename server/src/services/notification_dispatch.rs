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
//!    to the operator-configured webhook URL in one of five formats
//!    (`generic`/`ntfy`/`gotify`/`discord`/`slack`) with HMAC-SHA256 signing
//!    and exponential-backoff retry (1s, 5s, 30s, 2m, 10m with full jitter).
//! 4. **Push fan-out (fire-and-forget)** — Spawns a background task to deliver
//!    minimized FCM/APNs/UnifiedPush payloads to active registered devices and
//!    deactivates provider-revoked tokens.
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

use chrono::Utc;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use reqwest::StatusCode;
use ring::hmac::{HMAC_SHA256, Key};
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

const FETCH_ACTIVE_PUSH_DEVICES_SQL: &str = r#"
    SELECT id, provider, token
    FROM user_push_devices
    WHERE user_id = $1 AND provider = $2 AND is_active = true
    ORDER BY last_seen_at DESC NULLS LAST, created_at DESC
"#;

const INVALIDATE_PUSH_DEVICE_SQL: &str = r#"
    UPDATE user_push_devices
    SET is_active = false,
        invalidated_at = COALESCE(invalidated_at, now()),
        updated_at = now()
    WHERE id = $1
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
}

impl ChannelStatus {
    fn as_str(&self) -> &'static str {
        match self {
            ChannelStatus::Delivered => "delivered",
            ChannelStatus::Pending => "pending",
            ChannelStatus::Skipped => "skipped",
            ChannelStatus::Failed => "failed",
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
/// 7. Spawns a background task for mobile push delivery (if configured + enabled)
/// 8. Returns a [`DispatchResult`] with per-channel status
pub async fn dispatch(
    state: &AppState,
    input: &NotificationInput,
) -> Result<DispatchResult, DispatchError> {
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
    let (rendered_title, rendered_body) = render_notification(
        &nt.in_app_template,
        &locale,
        &input.metadata,
        &input.title,
        &input.body,
    );

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

    let sse_status = publish_sse(
        state,
        input.user_id,
        notification_id,
        &nt,
        &rendered_title,
        &rendered_body,
        input,
    );

    let in_app_status = ChannelStatus::Delivered;

    let push_status = dispatch_push(
        state,
        input.user_id,
        notification_id,
        &nt,
        &prefs,
        &rendered_title,
        &rendered_body,
        input,
    );

    let webhook_status = if prefs.webhook_enabled
        && state
            .runtime_config
            .load()
            .notifications
            .webhook
            .is_configured()
    {
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

    record_notification_delivery("in_app", &in_app_status);
    record_notification_delivery("sse", &sse_status);
    record_notification_delivery("webhook", &webhook_status);
    record_notification_delivery("push", &push_status);

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

    state
        .event_bus
        .publish(user_id, ServerEvent::new("notification", payload));
    ChannelStatus::Delivered
}

fn dispatch_push(
    state: &AppState,
    user_id: Uuid,
    notification_id: Uuid,
    nt: &NotificationTypeInfo,
    prefs: &UserPrefs,
    title: &str,
    body: &str,
    input: &NotificationInput,
) -> ChannelStatus {
    let config = state.runtime_config.load();
    if !prefs.push_enabled {
        return ChannelStatus::Skipped;
    }
    if !config.notifications.push.is_configured() {
        return ChannelStatus::Skipped;
    }

    let push_config = config.notifications.push.clone();
    drop(config);

    spawn_push_delivery(
        state.pool.clone(),
        user_id,
        notification_id,
        push_config,
        PushNotificationPayload::new(notification_id, nt, title, body, input),
    );
    ChannelStatus::Pending
}

#[derive(Debug, Clone)]
struct PushNotificationPayload {
    notification_id: Uuid,
    notification_type: String,
    title: String,
    body: String,
    link: Option<String>,
    related_item_id: Option<Uuid>,
}

impl PushNotificationPayload {
    fn new(
        notification_id: Uuid,
        _nt: &NotificationTypeInfo,
        title: &str,
        body: &str,
        input: &NotificationInput,
    ) -> Self {
        Self {
            notification_id,
            notification_type: input.notification_type.clone(),
            title: title.to_string(),
            body: body.to_string(),
            link: input.link.clone(),
            related_item_id: input.related_item_id,
        }
    }

    fn data(&self) -> serde_json::Map<String, Value> {
        let mut data = serde_json::Map::new();
        data.insert(
            "notification_id".to_string(),
            Value::String(self.notification_id.to_string()),
        );
        data.insert(
            "type".to_string(),
            Value::String(self.notification_type.clone()),
        );
        if let Some(link) = &self.link {
            data.insert("link".to_string(), Value::String(link.clone()));
        }
        if let Some(id) = self.related_item_id {
            data.insert("related_item_id".to_string(), Value::String(id.to_string()));
        }
        data
    }
}

#[derive(Debug)]
struct PushDeviceTarget {
    id: Uuid,
    token: String,
}

fn spawn_push_delivery(
    pool: PgPool,
    user_id: Uuid,
    notification_id: Uuid,
    config: crate::state::PushDispatchConfig,
    payload: PushNotificationPayload,
) {
    tokio::spawn(async move {
        let provider = match config.provider.as_deref() {
            Some(provider) => provider.to_string(),
            None => {
                update_notification_channel_status(&pool, notification_id, "push", "skipped").await;
                return;
            }
        };

        let devices = match fetch_active_push_devices(&pool, user_id, &provider).await {
            Ok(devices) => devices,
            Err(error) => {
                tracing::warn!(
                    notification_id = %notification_id,
                    provider = %provider,
                    error = %error,
                    "Failed to fetch push devices"
                );
                update_notification_channel_status(&pool, notification_id, "push", "failed").await;
                record_notification_delivery_status("push", "failed");
                return;
            }
        };

        if devices.is_empty() {
            update_notification_channel_status(&pool, notification_id, "push", "skipped").await;
            record_notification_delivery_status("push", "skipped");
            return;
        }

        let mut delivered = 0usize;
        let mut failed = 0usize;
        let mut invalidated = 0usize;

        for device in devices {
            let result = send_push_to_device(&config, &provider, &device.token, &payload).await;
            match result {
                Ok(()) => delivered += 1,
                Err(PushDeliveryError::Revoked(reason)) => {
                    invalidated += 1;
                    failed += 1;
                    let _ = sqlx::query(INVALIDATE_PUSH_DEVICE_SQL)
                        .bind(device.id)
                        .execute(&pool)
                        .await;
                    tracing::info!(
                        notification_id = %notification_id,
                        provider = %provider,
                        reason = %reason,
                        "Push provider revoked a device token; device row invalidated"
                    );
                }
                Err(error) => {
                    failed += 1;
                    tracing::warn!(
                        notification_id = %notification_id,
                        provider = %provider,
                        error = %error,
                        "Push delivery failed for a registered device"
                    );
                }
            }
        }

        let status = if delivered > 0 {
            "delivered"
        } else if failed > 0 {
            "failed"
        } else {
            "skipped"
        };

        tracing::debug!(
            notification_id = %notification_id,
            provider = %provider,
            delivered,
            failed,
            invalidated,
            "Push delivery batch completed"
        );

        update_notification_channel_status(&pool, notification_id, "push", status).await;
        record_notification_delivery_status("push", status);
    });
}

async fn fetch_active_push_devices(
    pool: &PgPool,
    user_id: Uuid,
    provider: &str,
) -> Result<Vec<PushDeviceTarget>, sqlx::Error> {
    let rows = sqlx::query(FETCH_ACTIVE_PUSH_DEVICES_SQL)
        .bind(user_id)
        .bind(provider)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| PushDeviceTarget {
            id: row.try_get("id").unwrap_or_else(|_| Uuid::nil()),
            token: row.try_get("token").unwrap_or_default(),
        })
        .collect())
}

async fn send_push_to_device(
    config: &crate::state::PushDispatchConfig,
    provider: &str,
    token: &str,
    payload: &PushNotificationPayload,
) -> Result<(), PushDeliveryError> {
    match provider {
        "fcm" => send_fcm_push(&config.fcm, token, payload).await,
        "apns" => send_apns_push(&config.apns, token, payload).await,
        "unifiedpush" => send_unifiedpush(token, payload).await,
        _ => Err(PushDeliveryError::Config(format!(
            "unsupported provider '{provider}'"
        ))),
    }
}

#[derive(Debug, thiserror::Error)]
enum PushDeliveryError {
    #[error("Push configuration error: {0}")]
    Config(String),
    #[error("Push token revoked: {0}")]
    Revoked(String),
    #[error("Push request failed: {0}")]
    Request(String),
    #[error("Push provider returned status {status}: {reason}")]
    Provider { status: u16, reason: String },
}

#[derive(Debug, Serialize)]
struct OAuthJwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: usize,
    exp: usize,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

async fn send_fcm_push(
    config: &crate::state::FcmPushConfig,
    token: &str,
    payload: &PushNotificationPayload,
) -> Result<(), PushDeliveryError> {
    let project_id = required_config(config.project_id.as_deref(), "fcm.project_id")?;
    let client_email = required_config(config.client_email.as_deref(), "fcm.client_email")?;
    let private_key = required_config(config.private_key.as_deref(), "fcm.private_key")?;
    let access_token = fetch_fcm_access_token(client_email, private_key).await?;

    let data = payload
        .data()
        .into_iter()
        .filter_map(|(key, value)| value.as_str().map(|s| (key, Value::String(s.to_string()))))
        .collect::<serde_json::Map<String, Value>>();

    let request_body = serde_json::json!({
        "message": {
            "token": token,
            "notification": {
                "title": payload.title,
                "body": payload.body,
            },
            "data": data,
        }
    });

    let url = format!("https://fcm.googleapis.com/v1/projects/{project_id}/messages:send");
    let client = push_http_client()?;
    let response = client
        .post(url)
        .bearer_auth(access_token)
        .header("User-Agent", "Duskcue-Push/1.0")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| PushDeliveryError::Request(e.to_string()))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if is_fcm_revoked(status, &body) {
        return Err(PushDeliveryError::Revoked("UNREGISTERED".to_string()));
    }
    Err(PushDeliveryError::Provider {
        status: status.as_u16(),
        reason: sanitized_provider_reason(&body),
    })
}

async fn fetch_fcm_access_token(
    client_email: &str,
    private_key: &str,
) -> Result<String, PushDeliveryError> {
    let now = Utc::now().timestamp() as usize;
    let claims = OAuthJwtClaims {
        iss: client_email,
        scope: "https://www.googleapis.com/auth/firebase.messaging",
        aud: "https://oauth2.googleapis.com/token",
        iat: now,
        exp: now + 3600,
    };
    let jwt = encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(&normalize_pem(private_key))
            .map_err(|e| PushDeliveryError::Config(e.to_string()))?,
    )
    .map_err(|e| PushDeliveryError::Config(e.to_string()))?;

    let client = push_http_client()?;
    let response = client
        .post("https://oauth2.googleapis.com/token")
        .header("User-Agent", "Duskcue-Push/1.0")
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", jwt.as_str()),
        ])
        .send()
        .await
        .map_err(|e| PushDeliveryError::Request(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(PushDeliveryError::Provider {
            status,
            reason: sanitized_provider_reason(&body),
        });
    }

    let body: OAuthTokenResponse = response
        .json()
        .await
        .map_err(|e| PushDeliveryError::Request(e.to_string()))?;
    Ok(body.access_token)
}

#[derive(Debug, Serialize)]
struct ApnsJwtClaims<'a> {
    iss: &'a str,
    iat: usize,
}

async fn send_apns_push(
    config: &crate::state::ApnsPushConfig,
    token: &str,
    payload: &PushNotificationPayload,
) -> Result<(), PushDeliveryError> {
    let team_id = required_config(config.team_id.as_deref(), "apns.team_id")?;
    let key_id = required_config(config.key_id.as_deref(), "apns.key_id")?;
    let private_key = required_config(config.private_key.as_deref(), "apns.private_key")?;
    let bundle_id = required_config(config.bundle_id.as_deref(), "apns.bundle_id")?;
    let now = Utc::now().timestamp() as usize;
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some(key_id.to_string());
    let jwt = encode(
        &header,
        &ApnsJwtClaims {
            iss: team_id,
            iat: now,
        },
        &EncodingKey::from_ec_pem(&normalize_pem(private_key))
            .map_err(|e| PushDeliveryError::Config(e.to_string()))?,
    )
    .map_err(|e| PushDeliveryError::Config(e.to_string()))?;

    let host = if config.sandbox {
        "api.sandbox.push.apple.com"
    } else {
        "api.push.apple.com"
    };
    let url = format!("https://{host}/3/device/{token}");
    let request_body = serde_json::json!({
        "aps": {
            "alert": {
                "title": payload.title,
                "body": payload.body,
            },
            "sound": "default",
            "thread-id": payload.notification_type,
        },
        "notification_id": payload.notification_id.to_string(),
        "type": payload.notification_type,
        "link": payload.link,
        "related_item_id": payload.related_item_id.map(|id| id.to_string()),
    });

    let client = push_http_client()?;
    let response = client
        .post(url)
        .bearer_auth(jwt)
        .header("User-Agent", "Duskcue-Push/1.0")
        .header("apns-topic", bundle_id)
        .header("apns-push-type", "alert")
        .header("apns-priority", "10")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| PushDeliveryError::Request(e.to_string()))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let reason = apns_reason(&body);
    if status == StatusCode::GONE
        || matches!(reason.as_deref(), Some("BadDeviceToken" | "Unregistered"))
    {
        return Err(PushDeliveryError::Revoked(
            reason.unwrap_or_else(|| status.as_str().to_string()),
        ));
    }
    Err(PushDeliveryError::Provider {
        status: status.as_u16(),
        reason: sanitized_provider_reason(&body),
    })
}

async fn send_unifiedpush(
    endpoint: &str,
    payload: &PushNotificationPayload,
) -> Result<(), PushDeliveryError> {
    let request_body = serde_json::json!({
        "notification_id": payload.notification_id,
        "type": payload.notification_type,
        "title": payload.title,
        "body": payload.body,
        "link": payload.link,
        "related_item_id": payload.related_item_id,
    });
    let client = push_http_client()?;
    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("User-Agent", "Duskcue-Push/1.0")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| PushDeliveryError::Request(e.to_string()))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE) {
        return Err(PushDeliveryError::Revoked(status.as_str().to_string()));
    }
    Err(PushDeliveryError::Provider {
        status: status.as_u16(),
        reason: sanitized_provider_reason(&body),
    })
}

fn push_http_client() -> Result<reqwest::Client, PushDeliveryError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| PushDeliveryError::Config(e.to_string()))
}

fn required_config<'a>(value: Option<&'a str>, field: &str) -> Result<&'a str, PushDeliveryError> {
    value
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| PushDeliveryError::Config(format!("{field} is required")))
}

fn normalize_pem(value: &str) -> Vec<u8> {
    value.replace("\\n", "\n").into_bytes()
}

fn is_fcm_revoked(status: StatusCode, body: &str) -> bool {
    status == StatusCode::NOT_FOUND && body.contains("UNREGISTERED")
}

fn apns_reason(body: &str) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(body).ok()?;
    parsed
        .get("reason")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

fn sanitized_provider_reason(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    compact.chars().take(200).collect()
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
    let format = WebhookFormat::from_config(&config.format);
    let url = match &config.url {
        Some(u) => u.clone(),
        None => return,
    };
    let secret = config.secret.clone();

    let mut req = format_request(format, &url, notification_id, nt, title, body, input);
    sign_request(&mut req, &secret);

    let format_label = config.format.clone();

    tokio::spawn(async move {
        let result = dispatch_webhook(&req).await;
        let status_str = match &result {
            Ok(()) => "delivered",
            Err(e) => {
                tracing::warn!(
                    notification_id = %notification_id,
                    url = %url,
                    format = %format_label,
                    error = %e,
                    attempts = WEBHOOK_BACKOFF_SECONDS.len() + 1,
                    "Webhook delivery exhausted retries; marked failed"
                );
                "failed"
            }
        };
        record_notification_delivery_status("webhook", status_str);

        update_notification_channel_status(&pool, notification_id, "webhook", status_str).await;
    });
}

async fn update_notification_channel_status(
    pool: &PgPool,
    notification_id: Uuid,
    channel: &str,
    status: &str,
) {
    let current_status: Value =
        match sqlx::query("SELECT delivery_status FROM notifications WHERE id = $1")
            .bind(notification_id)
            .fetch_one(pool)
            .await
        {
            Ok(row) => row.try_get("delivery_status").unwrap_or(Value::Null),
            Err(_) => Value::Null,
        };

    let mut status_map = match current_status {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    status_map.insert(channel.to_string(), Value::String(status.to_string()));

    let _ = sqlx::query("UPDATE notifications SET delivery_status = $2 WHERE id = $1")
        .bind(notification_id)
        .bind(Value::Object(status_map))
        .execute(pool)
        .await;
}

fn record_notification_delivery(channel: &str, status: &ChannelStatus) {
    record_notification_delivery_status(channel, status.as_str());
}

fn record_notification_delivery_status(channel: &str, status: &str) {
    metrics::counter!(
        "notification_delivery_total",
        "channel" => channel.to_string(),
        "status" => status.to_string()
    )
    .increment(1);
}

/// Supported webhook payload formats. Selected via
/// `server_config.notifications.webhook.format`. Unknown config values fall
/// back to [`WebhookFormat::Generic`] so a typo never breaks dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookFormat {
    Generic,
    Ntfy,
    Gotify,
    Discord,
    Slack,
}

impl WebhookFormat {
    /// Parse a format string from config. Unknown values fall back to
    /// [`WebhookFormat::Generic`] (logged at INFO by the caller) so a typo
    /// never breaks dispatch. Infallible by design — does not implement
    /// `FromStr` (which would require returning `Result`).
    pub fn from_config(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "ntfy" => Self::Ntfy,
            "gotify" => Self::Gotify,
            "discord" => Self::Discord,
            "slack" => Self::Slack,
            _ => Self::Generic,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Ntfy => "ntfy",
            Self::Gotify => "gotify",
            Self::Discord => "discord",
            Self::Slack => "slack",
        }
    }
}

/// A fully-formed webhook request, ready to send. Produced by
/// [`format_request`] per the selected [`WebhookFormat`].
struct FormattedRequest {
    /// URL to POST to. May differ from the operator's base URL (e.g. Discord
    /// appends `?wait=true` so the response carries a real status code).
    url: String,
    /// `Content-Type` header value.
    content_type: &'static str,
    /// Additional headers (format-specific `Title`/`Priority` for ntfy, plus
    /// the `X-Duskcue-Signature` HMAC header added by [`sign_request`]).
    headers: Vec<(String, String)>,
    /// Serialized request body bytes. The HMAC signature (when a secret is
    /// configured) is computed over these exact bytes.
    body: Vec<u8>,
}

fn format_request(
    format: WebhookFormat,
    url: &str,
    notification_id: Uuid,
    nt: &NotificationTypeInfo,
    title: &str,
    body: &str,
    input: &NotificationInput,
) -> FormattedRequest {
    match format {
        WebhookFormat::Generic => {
            let payload = build_webhook_payload(notification_id, nt, title, body, input);
            FormattedRequest {
                url: url.to_string(),
                content_type: "application/json",
                headers: Vec::new(),
                body: serde_json::to_vec(&payload).unwrap_or_default(),
            }
        }
        WebhookFormat::Ntfy => {
            // ntfy takes a plain-text body with Title/Priority/Tags/Markdown headers
            // (https://docs.ntfy.sh/publish/). The body is the message text.
            let body_text = if title.is_empty() {
                body.to_string()
            } else {
                format!("{title}\n\n{body}")
            };
            FormattedRequest {
                url: url.to_string(),
                content_type: "text/plain; charset=utf-8",
                headers: vec![
                    ("Title".to_string(), title.to_string()),
                    (
                        "Priority".to_string(),
                        ntfy_priority(&nt.priority).to_string(),
                    ),
                    ("Tags".to_string(), ntfy_tags(&nt.category).to_string()),
                    ("Markdown".to_string(), "yes".to_string()),
                ],
                body: body_text.into_bytes(),
            }
        }
        WebhookFormat::Gotify => {
            // Gotify message JSON: {title, message, priority}. The app token is
            // part of the operator-configured URL (?token=...), so no auth header.
            let payload = serde_json::json!({
                "title": title,
                "message": body,
                "priority": gotify_priority(&nt.priority),
            });
            FormattedRequest {
                url: url.to_string(),
                content_type: "application/json",
                headers: Vec::new(),
                body: serde_json::to_vec(&payload).unwrap_or_default(),
            }
        }
        WebhookFormat::Discord => {
            // Discord caps `content` at 2000 chars. Compose a single message
            // with a bold title. ?wait=true ensures Discord returns a real
            // status (otherwise it replies 204 even for rate-limited drops).
            let composed = if title.is_empty() {
                body.to_string()
            } else {
                format!("**{title}**\n{body}")
            };
            let truncated: String = composed.chars().take(2000).collect();
            let payload = serde_json::json!({
                "username": "Duskcue",
                "content": truncated,
            });
            let sep = if url.contains('?') { '&' } else { '?' };
            FormattedRequest {
                url: format!("{url}{sep}wait=true"),
                content_type: "application/json",
                headers: Vec::new(),
                body: serde_json::to_vec(&payload).unwrap_or_default(),
            }
        }
        WebhookFormat::Slack => {
            // Slack incoming webhook: {text}. mrkdwn is on by default.
            let text = if title.is_empty() {
                body.to_string()
            } else {
                format!("*{title}*\n{body}")
            };
            let payload = serde_json::json!({ "text": text });
            FormattedRequest {
                url: url.to_string(),
                content_type: "application/json",
                headers: Vec::new(),
                body: serde_json::to_vec(&payload).unwrap_or_default(),
            }
        }
    }
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

/// Map a Duskcue priority (`low`/`medium`/`high`) to the ntfy 1-5 priority
/// scale (1=min, 2=low, 3=default, 4=high, 5=max).
fn ntfy_priority(priority: &str) -> u8 {
    match priority {
        "high" => 5,
        "medium" => 3,
        _ => 2,
    }
}

/// Map a Duskcue priority to the Gotify 0-10 priority scale.
fn gotify_priority(priority: &str) -> i32 {
    match priority {
        "high" => 8,
        "medium" => 5,
        _ => 2,
    }
}

/// ntfy emoji tag set per notification category (ntfy renders these as emoji).
fn ntfy_tags(category: &str) -> &'static str {
    match category {
        "security" => "rotating_light,warning",
        "system" => "gear",
        "media" => "film_projector",
        "task" => "clipboard",
        "user" => "bust_in_silhouette",
        _ => "bell",
    }
}

/// Append the `X-Duskcue-Signature` HMAC-SHA256 header (computed over the
/// request body) when a shared secret is configured. Applied to all formats.
fn sign_request(req: &mut FormattedRequest, secret: &Option<String>) {
    if let Some(secret) = secret
        && !secret.is_empty()
    {
        let signature = compute_hmac_signature(secret.as_bytes(), &req.body);
        req.headers.push((
            "X-Duskcue-Signature".to_string(),
            format!("sha256={signature}"),
        ));
    }
}

/// Backoff schedule (seconds) between retries, per
/// [MOBILE_PUSH.md](../../docs/design/MOBILE_PUSH.md) §Retry policy.
/// The webhook is attempted once immediately, then retried up to
/// `WEBHOOK_BACKOFF_SECONDS.len()` times with these waits applied before each
/// retry (1s, 5s, 30s, 2m, 10m). Full jitter (0.5×–1.5×) is applied to every
/// wait to avoid thundering-herd spikes when many notifications fail at once.
const WEBHOOK_BACKOFF_SECONDS: [u64; 5] = [1, 5, 30, 120, 600];

/// HTTP status codes worth retrying. All 4xx (except 429) are treated as
/// permanent failures (bad URL, revoked token, deleted webhook). See
/// MOBILE_PUSH.md §Retry policy and the Hookdeck/Svix retry best-practice
/// guides cited in MOBILE_PUSH.md Research Sources.
fn is_retryable_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504)
}

/// Parse a `Retry-After` header value. Supports the integer-seconds form
/// (`Retry-After: 120`) used by ntfy/Gotify/Discord/Slack. HTTP-date form is
/// ignored (returns `None`) — rare for these services.
fn parse_retry_after(header_value: &str) -> Option<std::time::Duration> {
    header_value
        .trim()
        .parse::<u64>()
        .ok()
        .map(std::time::Duration::from_secs)
}

/// Apply full jitter to a duration: returns a value in `[0.5×, 1.5×)` of the
/// input. Bounds the random factor so a 1s base never becomes a 0ms sleep.
fn jittered_duration(base: std::time::Duration) -> std::time::Duration {
    let factor = 0.5 + rand::random::<f64>();
    let millis = (base.as_millis() as f64 * factor) as u64;
    std::time::Duration::from_millis(millis.max(1))
}

fn build_webhook_client() -> Result<reqwest::Client, WebhookError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .connect_timeout(std::time::Duration::from_secs(10))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| WebhookError::ClientBuild(e.to_string()))
}

async fn dispatch_webhook(req: &FormattedRequest) -> Result<(), WebhookError> {
    let client = build_webhook_client()?;

    // Initial attempt (no preceding wait), then up to WEBHOOK_BACKOFF_SECONDS
    // retries with jittered exponential backoff.
    for attempt in 0..=WEBHOOK_BACKOFF_SECONDS.len() {
        if attempt > 0 {
            let backoff = WEBHOOK_BACKOFF_SECONDS[attempt - 1];
            let delay = jittered_duration(std::time::Duration::from_secs(backoff));
            tracing::debug!(
                attempt,
                backoff_seconds = backoff,
                delay_ms = delay.as_millis(),
                "Webhook backing off before retry"
            );
            tokio::time::sleep(delay).await;
        }

        match send_once(&client, req).await {
            Ok(()) => return Ok(()),
            Err(e @ WebhookError::NonRetryableStatus { .. }) => {
                tracing::warn!(
                    attempt,
                    error = %e,
                    "Webhook returned non-retryable status; not retrying"
                );
                return Err(e);
            }
            Err(WebhookError::RetryableStatus {
                retry_after,
                status,
                ..
            }) => {
                if let Some(ra) = retry_after {
                    // 429 with Retry-After: honor it (capped at 10 minutes so a
                    // malicious or misconfigured endpoint can't stall delivery).
                    let capped = std::cmp::min(ra, std::time::Duration::from_secs(600));
                    tracing::warn!(
                        attempt,
                        status,
                        retry_after_ms = capped.as_millis(),
                        "Webhook rate-limited (429); honoring Retry-After"
                    );
                    tokio::time::sleep(jittered_duration(capped)).await;
                } else {
                    tracing::warn!(
                        attempt,
                        status,
                        "Webhook returned retryable status; will back off"
                    );
                }
            }
            Err(WebhookError::ClientBuild(_)) => {
                // Cannot recur (client already built above); treat as terminal.
                unreachable!("client built once before loop")
            }
            Err(e @ WebhookError::RequestFailed(_)) => {
                tracing::warn!(
                    attempt,
                    error = %e,
                    "Webhook network/timeout error; will retry"
                );
            }
        }
    }

    Err(WebhookError::RequestFailed(
        "exhausted all retry attempts".to_string(),
    ))
}

async fn send_once(client: &reqwest::Client, req: &FormattedRequest) -> Result<(), WebhookError> {
    let mut request = client
        .post(&req.url)
        .header("Content-Type", req.content_type)
        .header("User-Agent", "Duskcue-Notifications/1.0");

    for (name, value) in &req.headers {
        request = request.header(name, value);
    }

    let response = request
        .body(req.body.clone())
        .send()
        .await
        .map_err(|e| WebhookError::RequestFailed(e.to_string()))?;

    let status = response.status();
    if status.is_success() {
        return Ok(());
    }

    let status_code = status.as_u16();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after);
    let body_text = response.text().await.unwrap_or_default();

    if is_retryable_status(status_code) {
        Err(WebhookError::RetryableStatus {
            status: status_code,
            retry_after,
            body: body_text,
        })
    } else {
        Err(WebhookError::NonRetryableStatus {
            status: status_code,
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
    #[error("Webhook returned non-retryable status {status}: {body}")]
    NonRetryableStatus { status: u16, body: String },
    #[error("Webhook returned retryable status {status}")]
    RetryableStatus {
        status: u16,
        retry_after: Option<std::time::Duration>,
        body: String,
    },
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
        category: row
            .try_get("category")
            .unwrap_or_else(|_| "system".to_string()),
        priority: row
            .try_get("priority")
            .unwrap_or_else(|_| "low".to_string()),
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
    }

    #[test]
    fn push_payload_data_is_minimized() {
        let notification_id = Uuid::now_v7();
        let related_item_id = Uuid::now_v7();
        let nt = NotificationTypeInfo {
            id: Uuid::now_v7(),
            category: "media".to_string(),
            priority: "medium".to_string(),
            in_app_template: "new-media-added".to_string(),
            is_enabled_by_default: true,
        };
        let mut input =
            NotificationInput::new(Uuid::now_v7(), "new_media_added", serde_json::json!({}));
        input.link = Some(format!("/media/{related_item_id}"));
        input.related_item_id = Some(related_item_id);

        let payload = PushNotificationPayload::new(notification_id, &nt, "Title", "Body", &input);
        let data = payload.data();

        assert_eq!(
            data.get("notification_id"),
            Some(&Value::String(notification_id.to_string()))
        );
        assert_eq!(
            data.get("type").and_then(Value::as_str),
            Some("new_media_added")
        );
        assert_eq!(
            data.get("link"),
            Some(&Value::String(format!("/media/{related_item_id}")))
        );
        assert_eq!(
            data.get("related_item_id"),
            Some(&Value::String(related_item_id.to_string()))
        );
        assert!(!data.contains_key("title"));
        assert!(!data.contains_key("body"));
        assert!(!data.contains_key("metadata"));
    }

    #[test]
    fn fcm_revocation_detection_requires_unregistered_404() {
        assert!(is_fcm_revoked(
            StatusCode::NOT_FOUND,
            r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#
        ));
        assert!(!is_fcm_revoked(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"details":[{"errorCode":"UNREGISTERED"}]}}"#
        ));
        assert!(!is_fcm_revoked(
            StatusCode::NOT_FOUND,
            r#"{"error":{"status":"NOT_FOUND"}}"#
        ));
    }

    #[test]
    fn apns_reason_extracts_provider_reason() {
        assert_eq!(
            apns_reason(r#"{"reason":"BadDeviceToken"}"#).as_deref(),
            Some("BadDeviceToken")
        );
        assert_eq!(apns_reason("not-json"), None);
    }

    #[test]
    fn normalize_pem_converts_json_escaped_newlines() {
        assert_eq!(
            normalize_pem("-----BEGIN-----\\nabc\\n-----END-----"),
            b"-----BEGIN-----\nabc\n-----END-----".to_vec()
        );
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

    fn sample_nt(category: &str, priority: &str) -> NotificationTypeInfo {
        NotificationTypeInfo {
            id: Uuid::nil(),
            category: category.to_string(),
            priority: priority.to_string(),
            in_app_template: "new-media-added".to_string(),
            is_enabled_by_default: true,
        }
    }

    #[test]
    fn webhook_format_parses_known_values_case_insensitively() {
        assert_eq!(
            WebhookFormat::from_config("generic"),
            WebhookFormat::Generic
        );
        assert_eq!(WebhookFormat::from_config("NTFY"), WebhookFormat::Ntfy);
        assert_eq!(WebhookFormat::from_config("Gotify"), WebhookFormat::Gotify);
        assert_eq!(
            WebhookFormat::from_config("discord"),
            WebhookFormat::Discord
        );
        assert_eq!(WebhookFormat::from_config(" slack "), WebhookFormat::Slack);
    }

    #[test]
    fn webhook_format_unknown_falls_back_to_generic() {
        assert_eq!(
            WebhookFormat::from_config("telegram"),
            WebhookFormat::Generic
        );
        assert_eq!(WebhookFormat::from_config(""), WebhookFormat::Generic);
    }

    #[test]
    fn generic_format_produces_json_with_full_payload() {
        let nt = sample_nt("media", "high");
        let input = NotificationInput::new(Uuid::nil(), "new_media_added", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Generic,
            "https://example.com/hook",
            Uuid::nil(),
            &nt,
            "Title",
            "Body",
            &input,
        );

        assert_eq!(req.content_type, "application/json");
        assert!(req.url.ends_with("/hook"));
        let parsed: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(parsed["title"], "Title");
        assert_eq!(parsed["body"], "Body");
        assert_eq!(parsed["category"], "media");
        assert!(parsed["notification_id"].is_string());
    }

    #[test]
    fn ntfy_format_uses_plain_text_with_priority_headers() {
        let nt = sample_nt("security", "high");
        let input = NotificationInput::new(Uuid::nil(), "trust_alert", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Ntfy,
            "https://ntfy.example.com/topic",
            Uuid::nil(),
            &nt,
            "Alert",
            "Suspicious login",
            &input,
        );

        assert_eq!(req.content_type, "text/plain; charset=utf-8");
        let hdrs: std::collections::HashMap<&str, &str> = req
            .headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(hdrs.get("Title"), Some(&"Alert"));
        assert_eq!(hdrs.get("Priority"), Some(&"5")); // high → 5
        assert_eq!(hdrs.get("Markdown"), Some(&"yes"));
        assert_eq!(hdrs.get("Tags"), Some(&"rotating_light,warning")); // security category
        let body = String::from_utf8(req.body).unwrap();
        assert!(body.contains("Alert"));
        assert!(body.contains("Suspicious login"));
    }

    #[test]
    fn ntfy_priority_maps_correctly() {
        assert_eq!(ntfy_priority("high"), 5);
        assert_eq!(ntfy_priority("medium"), 3);
        assert_eq!(ntfy_priority("low"), 2);
        assert_eq!(ntfy_priority("unknown"), 2);
    }

    #[test]
    fn gotify_format_produces_message_json_with_priority() {
        let nt = sample_nt("media", "medium");
        let input = NotificationInput::new(Uuid::nil(), "new_media_added", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Gotify,
            "https://gotify.example.com/message?token=Aabc123",
            Uuid::nil(),
            &nt,
            "New Media",
            "Body text",
            &input,
        );

        assert_eq!(req.content_type, "application/json");
        // URL preserved (token stays in query string, no auth header added)
        assert!(req.url.contains("token=Aabc123"));
        let parsed: Value = serde_json::from_slice(&req.body).unwrap();
        assert_eq!(parsed["title"], "New Media");
        assert_eq!(parsed["message"], "Body text");
        assert_eq!(parsed["priority"], 5); // medium → 5
    }

    #[test]
    fn discord_format_truncates_content_and_appends_wait() {
        let nt = sample_nt("media", "low");
        let long_body: String = "x".repeat(3000);
        let input = NotificationInput::new(Uuid::nil(), "new_media_added", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Discord,
            "https://discord.com/api/webhooks/123/abc",
            Uuid::nil(),
            &nt,
            "T",
            &long_body,
            &input,
        );

        let parsed: Value = serde_json::from_slice(&req.body).unwrap();
        let content = parsed["content"].as_str().unwrap();
        assert!(content.chars().count() <= 2000);
        assert_eq!(parsed["username"], "Duskcue");
        // ?wait=true appended (no existing query)
        assert!(req.url.ends_with("?wait=true"));
    }

    #[test]
    fn discord_format_preserves_existing_query_when_appending_wait() {
        let nt = sample_nt("media", "low");
        let input = NotificationInput::new(Uuid::nil(), "t", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Discord,
            "https://discord.com/api/webhooks/123/abc?thread_id=99",
            Uuid::nil(),
            &nt,
            "T",
            "B",
            &input,
        );
        assert!(req.url.contains("&wait=true"));
        assert!(req.url.contains("thread_id=99"));
    }

    #[test]
    fn slack_format_produces_text_payload() {
        let nt = sample_nt("system", "high");
        let input = NotificationInput::new(Uuid::nil(), "backup_failed", serde_json::json!({}));
        let req = format_request(
            WebhookFormat::Slack,
            "https://hooks.slack.com/services/T/B/X",
            Uuid::nil(),
            &nt,
            "Backup Failed",
            "See logs",
            &input,
        );

        let parsed: Value = serde_json::from_slice(&req.body).unwrap();
        assert!(parsed["text"].as_str().unwrap().contains("*Backup Failed*"));
        assert!(parsed["text"].as_str().unwrap().contains("See logs"));
    }

    #[test]
    fn sign_request_appends_hmac_header_when_secret_present() {
        let nt = sample_nt("media", "low");
        let input = NotificationInput::new(Uuid::nil(), "t", serde_json::json!({}));
        let mut req = format_request(
            WebhookFormat::Generic,
            "https://example.com/h",
            Uuid::nil(),
            &nt,
            "T",
            "B",
            &input,
        );
        assert!(req.headers.is_empty());

        sign_request(&mut req, &Some("shared-secret".to_string()));

        let sig_header = req
            .headers
            .iter()
            .find(|(k, _)| k == "X-Duskcue-Signature")
            .expect("signature header missing");
        assert!(sig_header.1.starts_with("sha256="));
        // Signature length: "sha256=" + 64 hex chars
        assert_eq!(sig_header.1.len(), "sha256=".len() + 64);
    }

    #[test]
    fn sign_request_skips_when_secret_absent_or_empty() {
        let nt = sample_nt("media", "low");
        let input = NotificationInput::new(Uuid::nil(), "t", serde_json::json!({}));
        let mut req = format_request(
            WebhookFormat::Generic,
            "https://example.com/h",
            Uuid::nil(),
            &nt,
            "T",
            "B",
            &input,
        );
        sign_request(&mut req, &None);
        assert!(req.headers.is_empty());

        sign_request(&mut req, &Some(String::new()));
        assert!(req.headers.is_empty());
    }

    #[test]
    fn retryable_status_classification_matches_best_practices() {
        // Retryable (transient)
        for code in [408, 429, 500, 502, 503, 504] {
            assert!(is_retryable_status(code), "{code} should be retryable");
        }
        // Non-retryable (permanent client errors)
        for code in [400, 401, 403, 404, 405, 410, 422] {
            assert!(!is_retryable_status(code), "{code} should NOT be retryable");
        }
        // 2xx not classified as retryable (they're successes, handled separately)
        assert!(!is_retryable_status(200));
    }

    #[test]
    fn parse_retry_after_accepts_integer_seconds() {
        assert_eq!(
            parse_retry_after("120"),
            Some(std::time::Duration::from_secs(120))
        );
        assert_eq!(
            parse_retry_after("  5 "),
            Some(std::time::Duration::from_secs(5))
        );
    }

    #[test]
    fn parse_retry_after_rejects_non_integer_and_http_date() {
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
        assert_eq!(parse_retry_after("not-a-number"), None);
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn jittered_duration_stays_within_half_to_one_and_a_half_band() {
        let base = std::time::Duration::from_secs(10);
        for _ in 0..200 {
            let jittered = jittered_duration(base);
            assert!(
                jittered >= std::time::Duration::from_millis(5000)
                    && jittered < std::time::Duration::from_millis(15000),
                "jittered duration {jittered:?} out of [5s, 15s) band"
            );
        }
    }

    #[test]
    fn backoff_schedule_matches_mobile_push_doc() {
        // 1s, 5s, 30s, 2m, 10m per MOBILE_PUSH.md §Retry policy
        assert_eq!(WEBHOOK_BACKOFF_SECONDS, [1, 5, 30, 120, 600]);
    }

    #[tokio::test]
    async fn send_once_classifies_429_as_retryable() {
        // Mock a 429 response by using a server that always returns 429.
        // We test send_once directly against a mockito-style listener.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/hook");

        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await.unwrap();
            let resp =
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 2\r\nContent-Length: 0\r\n\r\n";
            sock.write_all(resp.as_bytes()).await.unwrap();
        });

        let client = build_webhook_client().unwrap();
        let req = FormattedRequest {
            url,
            content_type: "application/json",
            headers: vec![],
            body: br#"{"x":1}"#.to_vec(),
        };
        let err = send_once(&client, &req).await.unwrap_err();
        match err {
            WebhookError::RetryableStatus {
                status,
                retry_after,
                ..
            } => {
                assert_eq!(status, 429);
                assert_eq!(retry_after, Some(std::time::Duration::from_secs(2)));
            }
            other => panic!("expected RetryableStatus, got {other:?}"),
        }
        server.await.unwrap();
    }

    #[tokio::test]
    async fn send_once_classifies_404_as_non_retryable() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{addr}/hook");

        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut sock, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = sock.read(&mut buf).await.unwrap();
            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nnot found";
            sock.write_all(resp.as_bytes()).await.unwrap();
        });

        let client = build_webhook_client().unwrap();
        let req = FormattedRequest {
            url,
            content_type: "application/json",
            headers: vec![],
            body: vec![],
        };
        let err = send_once(&client, &req).await.unwrap_err();
        match err {
            WebhookError::NonRetryableStatus { status, body } => {
                assert_eq!(status, 404);
                assert_eq!(body, "not found");
            }
            other => panic!("expected NonRetryableStatus, got {other:?}"),
        }
        server.await.unwrap();
    }
}
