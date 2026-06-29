// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even implied
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use base64::Engine;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::notifications::error::NotificationsError;
use crate::domains::notifications::types::*;

const LIST_NOTIFICATIONS_DESC_SQL: &str = r#"
    SELECT n.id, n.user_id, n.notification_type_id, n.title, n.body, n.priority,
           n.link, n.is_read, n.read_at, n.delivery_channels, n.delivery_status,
           n.related_item_type, n.related_item_id, n.expires_at, n.metadata,
           n.created_at, n.updated_at,
           nt.name AS notification_type_name, nt.category AS notification_type_category
    FROM notifications n
    JOIN notification_types nt ON nt.id = n.notification_type_id
    WHERE n.user_id = $1
      AND ($2::bool IS NULL OR n.is_read = $2)
      AND ($3::text IS NULL OR nt.category = $3)
      AND ($4::text IS NULL OR n.priority = $4)
      AND ($5::text IS NULL OR nt.name = $5)
      AND ($6::uuid IS NULL OR n.id < $6)
    ORDER BY n.id DESC
    LIMIT $7
"#;

const LIST_NOTIFICATIONS_ASC_SQL: &str = r#"
    SELECT n.id, n.user_id, n.notification_type_id, n.title, n.body, n.priority,
           n.link, n.is_read, n.read_at, n.delivery_channels, n.delivery_status,
           n.related_item_type, n.related_item_id, n.expires_at, n.metadata,
           n.created_at, n.updated_at,
           nt.name AS notification_type_name, nt.category AS notification_type_category
    FROM notifications n
    JOIN notification_types nt ON nt.id = n.notification_type_id
    WHERE n.user_id = $1
      AND ($2::bool IS NULL OR n.is_read = $2)
      AND ($3::text IS NULL OR nt.category = $3)
      AND ($4::text IS NULL OR n.priority = $4)
      AND ($5::text IS NULL OR nt.name = $5)
      AND ($6::uuid IS NULL OR n.id > $6)
    ORDER BY n.id ASC
    LIMIT $7
"#;

const COUNT_UNREAD_SQL: &str =
    "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND is_read = false";

const MARK_READ_SQL: &str = r#"
    UPDATE notifications SET is_read = true, read_at = now()
    WHERE id = $1 AND user_id = $2 AND is_read = false
    RETURNING read_at
"#;

const MARK_ALL_READ_SQL: &str = r#"
    UPDATE notifications SET is_read = true, read_at = now()
    WHERE user_id = $1 AND is_read = false
"#;

const DELETE_NOTIFICATION_SQL: &str = "DELETE FROM notifications WHERE id = $1 AND user_id = $2";

const DELETE_READ_SQL: &str = "DELETE FROM notifications WHERE user_id = $1 AND is_read = true";

const LIST_NOTIFICATION_TYPES_SQL: &str = r#"
    SELECT id, name, category, priority, in_app_template, is_enabled_by_default, created_at
    FROM notification_types
    ORDER BY category ASC, name ASC
"#;

const FETCH_NOTIFICATION_TYPE_SQL: &str = r#"
    SELECT id, name, category, priority, in_app_template, is_enabled_by_default, created_at
    FROM notification_types WHERE id = $1
"#;

const LIST_PREFERENCES_SQL: &str = r#"
    SELECT nt.id, nt.name, nt.category, nt.priority, nt.is_enabled_by_default,
           unp.in_app_enabled, unp.webhook_enabled, unp.push_enabled
    FROM notification_types nt
    LEFT JOIN user_notification_preferences unp
      ON unp.notification_type_id = nt.id AND unp.user_id = $1
    ORDER BY nt.category ASC, nt.name ASC
"#;

const UPSERT_PREFERENCE_SQL: &str = r#"
    INSERT INTO user_notification_preferences (id, user_id, notification_type_id,
        in_app_enabled, webhook_enabled, push_enabled)
    VALUES (uuidv7(), $1, $2, $3, $4, $5)
    ON CONFLICT (user_id, notification_type_id) DO UPDATE
    SET in_app_enabled = COALESCE($3, user_notification_preferences.in_app_enabled),
        webhook_enabled = COALESCE($4, user_notification_preferences.webhook_enabled),
        push_enabled = COALESCE($5, user_notification_preferences.push_enabled),
        updated_at = now()
    RETURNING in_app_enabled, webhook_enabled, push_enabled
"#;

pub async fn list_notifications(
    pool: &PgPool,
    user_id: Uuid,
    query: &NotificationListQuery,
) -> Result<NotificationListResponse, NotificationsError> {
    validate_category(query.category.as_deref())?;
    validate_priority(query.priority.as_deref())?;

    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let order = query.order.as_deref().unwrap_or("desc");
    let is_asc = order.eq_ignore_ascii_case("asc");
    let cursor_id = parse_cursor(query.cursor.as_deref());
    let fetch_limit = (limit + 1) as i64;

    let sql = if is_asc {
        LIST_NOTIFICATIONS_ASC_SQL
    } else {
        LIST_NOTIFICATIONS_DESC_SQL
    };

    let rows = sqlx::query(sql)
        .bind(user_id)
        .bind(query.is_read)
        .bind(query.category.as_deref())
        .bind(query.priority.as_deref())
        .bind(query.notification_type.as_deref())
        .bind(cursor_id)
        .bind(fetch_limit)
        .fetch_all(pool)
        .await?;

    let has_more = rows.len() > limit as usize;
    let rows = if has_more {
        &rows[..limit as usize]
    } else {
        &rows
    };

    let items: Vec<NotificationResponse> = rows.iter().map(row_to_response).collect();

    let next_cursor = if has_more {
        items.last().map(|i| encode_cursor(i.id))
    } else {
        None
    };

    Ok(NotificationListResponse {
        items,
        cursor: next_cursor,
        has_more,
    })
}

pub async fn count_unread(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<UnreadCountResponse, NotificationsError> {
    let count: i64 = sqlx::query_scalar(COUNT_UNREAD_SQL)
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(UnreadCountResponse {
        unread_count: count,
    })
}

pub async fn mark_read(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<MarkReadResponse, NotificationsError> {
    let read_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(MARK_READ_SQL)
        .bind(notification_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?;

    match read_at {
        Some(ts) => Ok(MarkReadResponse {
            notification_id,
            read: true,
            read_at: ts,
        }),
        None => {
            let exists: Option<Uuid> =
                sqlx::query_scalar("SELECT id FROM notifications WHERE id = $1 AND user_id = $2")
                    .bind(notification_id)
                    .bind(user_id)
                    .fetch_optional(pool)
                    .await?;
            if exists.is_some() {
                let now = chrono::Utc::now();
                Ok(MarkReadResponse {
                    notification_id,
                    read: true,
                    read_at: now,
                })
            } else {
                Err(NotificationsError::NotFound)
            }
        }
    }
}

pub async fn mark_all_read(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<BulkMarkReadResponse, NotificationsError> {
    let result = sqlx::query(MARK_ALL_READ_SQL)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(BulkMarkReadResponse {
        marked_read: result.rows_affected() as i64,
    })
}

pub async fn delete_notification(
    pool: &PgPool,
    user_id: Uuid,
    notification_id: Uuid,
) -> Result<DeleteResponse, NotificationsError> {
    let result = sqlx::query(DELETE_NOTIFICATION_SQL)
        .bind(notification_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(NotificationsError::NotFound);
    }
    Ok(DeleteResponse { deleted: true })
}

pub async fn delete_read(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<BulkDeleteResponse, NotificationsError> {
    let result = sqlx::query(DELETE_READ_SQL)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(BulkDeleteResponse {
        deleted: result.rows_affected() as i64,
    })
}

pub async fn list_notification_types(
    pool: &PgPool,
) -> Result<NotificationTypeListResponse, NotificationsError> {
    let rows = sqlx::query(LIST_NOTIFICATION_TYPES_SQL)
        .fetch_all(pool)
        .await?;
    let items: Vec<NotificationTypeResponse> = rows.iter().map(type_row_to_response).collect();
    Ok(NotificationTypeListResponse { items })
}

pub async fn list_preferences(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<NotificationPreferenceListResponse, NotificationsError> {
    let rows = sqlx::query(LIST_PREFERENCES_SQL)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    let preferences: Vec<NotificationPreferenceResponse> = rows
        .iter()
        .map(|r| NotificationPreferenceResponse {
            notification_type_id: r.try_get("id").unwrap_or_default(),
            name: r.try_get("name").unwrap_or_default(),
            category: r.try_get("category").unwrap_or_default(),
            priority: r.try_get("priority").unwrap_or_default(),
            is_enabled_by_default: r.try_get("is_enabled_by_default").unwrap_or(true),
            in_app_enabled: r.try_get("in_app_enabled").unwrap_or(true),
            webhook_enabled: r.try_get("webhook_enabled").unwrap_or(false),
            push_enabled: r.try_get("push_enabled").unwrap_or(false),
            is_using_defaults: r
                .try_get::<Option<bool>, _>("in_app_enabled")
                .ok()
                .flatten()
                .is_none(),
        })
        .collect();

    Ok(NotificationPreferenceListResponse { preferences })
}

pub async fn update_preference(
    pool: &PgPool,
    user_id: Uuid,
    notification_type_id: Uuid,
    req: &UpdatePreferenceRequest,
) -> Result<PreferenceUpdateResponse, NotificationsError> {
    let _ = fetch_notification_type(pool, notification_type_id).await?;

    let row = sqlx::query(UPSERT_PREFERENCE_SQL)
        .bind(user_id)
        .bind(notification_type_id)
        .bind(req.in_app_enabled)
        .bind(req.webhook_enabled)
        .bind(req.push_enabled)
        .fetch_one(pool)
        .await?;

    Ok(PreferenceUpdateResponse {
        notification_type_id,
        in_app_enabled: row.try_get("in_app_enabled").unwrap_or(true),
        webhook_enabled: row.try_get("webhook_enabled").unwrap_or(false),
        push_enabled: row.try_get("push_enabled").unwrap_or(false),
    })
}

async fn fetch_notification_type(
    pool: &PgPool,
    type_id: Uuid,
) -> Result<NotificationTypeRow, NotificationsError> {
    let row = sqlx::query(FETCH_NOTIFICATION_TYPE_SQL)
        .bind(type_id)
        .fetch_optional(pool)
        .await?
        .ok_or(NotificationsError::NotificationTypeNotFound)?;

    Ok(NotificationTypeRow {
        id: row.try_get("id").unwrap_or_default(),
        name: row.try_get("name").unwrap_or_default(),
        category: row.try_get("category").unwrap_or_default(),
        priority: row.try_get("priority").unwrap_or_default(),
        in_app_template: row.try_get("in_app_template").unwrap_or_default(),
        is_enabled_by_default: row.try_get("is_enabled_by_default").unwrap_or(true),
        created_at: row.try_get("created_at").unwrap_or_default(),
    })
}

fn row_to_response(r: &sqlx::postgres::PgRow) -> NotificationResponse {
    NotificationResponse {
        id: r.try_get("id").unwrap_or_default(),
        user_id: r.try_get("user_id").unwrap_or_default(),
        notification_type: r.try_get("notification_type_name").unwrap_or_default(),
        category: r.try_get("notification_type_category").unwrap_or_default(),
        title: r.try_get("title").unwrap_or_default(),
        body: r.try_get("body").unwrap_or_default(),
        priority: r.try_get("priority").unwrap_or_default(),
        link: r.try_get("link").ok().flatten(),
        is_read: r.try_get("is_read").unwrap_or(false),
        read_at: r.try_get("read_at").ok().flatten(),
        related_item_type: r.try_get("related_item_type").ok().flatten(),
        related_item_id: r.try_get("related_item_id").ok().flatten(),
        expires_at: r.try_get("expires_at").ok().flatten(),
        metadata: r.try_get("metadata").unwrap_or(serde_json::Value::Null),
        created_at: r.try_get("created_at").unwrap_or_default(),
    }
}

fn type_row_to_response(r: &sqlx::postgres::PgRow) -> NotificationTypeResponse {
    NotificationTypeResponse {
        id: r.try_get("id").unwrap_or_default(),
        name: r.try_get("name").unwrap_or_default(),
        category: r.try_get("category").unwrap_or_default(),
        priority: r.try_get("priority").unwrap_or_default(),
        in_app_template: r.try_get("in_app_template").unwrap_or_default(),
        is_enabled_by_default: r.try_get("is_enabled_by_default").unwrap_or(true),
        created_at: r.try_get("created_at").unwrap_or_default(),
    }
}

fn validate_category(category: Option<&str>) -> Result<(), NotificationsError> {
    if let Some(c) = category
        && !VALID_CATEGORIES.contains(&c)
    {
        return Err(NotificationsError::InvalidCategory(c.to_string()));
    }
    Ok(())
}

fn validate_priority(priority: Option<&str>) -> Result<(), NotificationsError> {
    if let Some(p) = priority
        && !VALID_PRIORITIES.contains(&p)
    {
        return Err(NotificationsError::InvalidPriority(p.to_string()));
    }
    Ok(())
}

fn parse_cursor(cursor: Option<&str>) -> Option<Uuid> {
    cursor.and_then(|c| {
        let bytes = base64::engine::general_purpose::STANDARD.decode(c).ok()?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        json.get("id")?
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok())
    })
}

fn encode_cursor(id: Uuid) -> String {
    let json = serde_json::json!({ "id": id.to_string() });
    base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&json).unwrap_or_default())
}

const UPSERT_PUSH_DEVICE_SQL: &str = r#"
    INSERT INTO user_push_devices (id, user_id, provider, token,
        device_name, platform, app_version, last_seen_at, is_active)
    VALUES (uuidv7(), $1, $2, $3, $4, $5, $6, now(), true)
    ON CONFLICT (user_id, provider, token) DO UPDATE
    SET last_seen_at = now(),
        is_active = true,
        invalidated_at = NULL,
        updated_at = now(),
        device_name = COALESCE(EXCLUDED.device_name, user_push_devices.device_name),
        platform = COALESCE(EXCLUDED.platform, user_push_devices.platform),
        app_version = COALESCE(EXCLUDED.app_version, user_push_devices.app_version)
    RETURNING id, user_id, provider, token, device_name, platform, app_version,
              last_seen_at, is_active, invalidated_at, created_at, updated_at
"#;

const LIST_PUSH_DEVICES_SQL: &str = r#"
    SELECT id, user_id, provider, token, device_name, platform, app_version,
           last_seen_at, is_active, invalidated_at, created_at, updated_at
    FROM user_push_devices
    WHERE user_id = $1
    ORDER BY is_active DESC, last_seen_at DESC NULLS LAST, created_at DESC
"#;

const UPDATE_PUSH_DEVICE_SQL: &str = r#"
    UPDATE user_push_devices
    SET last_seen_at = now(),
        updated_at = now(),
        device_name = COALESCE($3, device_name),
        platform = COALESCE($4, platform),
        app_version = COALESCE($5, app_version)
    WHERE id = $1 AND user_id = $2 AND is_active = true
    RETURNING id, user_id, provider, token, device_name, platform, app_version,
              last_seen_at, is_active, invalidated_at, created_at, updated_at
"#;

const DELETE_PUSH_DEVICE_SQL: &str = "DELETE FROM user_push_devices WHERE id = $1 AND user_id = $2";

const DEACTIVATE_STALE_DEVICES_SQL: &str = r#"
    UPDATE user_push_devices
    SET is_active = false,
        invalidated_at = COALESCE(invalidated_at, now()),
        updated_at = now()
    WHERE is_active = true
      AND last_seen_at IS NOT NULL
      AND last_seen_at < now() - ($1::INT * INTERVAL '1 day')
"#;

pub async fn register_push_device(
    pool: &PgPool,
    user_id: Uuid,
    req: &RegisterPushDeviceRequest,
) -> Result<PushDeviceResponse, NotificationsError> {
    validate_push_provider(&req.provider)?;
    validate_push_token(&req.provider, &req.token)?;
    validate_optional_length(
        req.device_name.as_deref(),
        MAX_DEVICE_NAME_LEN,
        "device_name",
    )?;
    validate_optional_length(req.platform.as_deref(), MAX_PLATFORM_LEN, "platform")?;
    validate_optional_length(
        req.app_version.as_deref(),
        MAX_APP_VERSION_LEN,
        "app_version",
    )?;

    let row = sqlx::query(UPSERT_PUSH_DEVICE_SQL)
        .bind(user_id)
        .bind(&req.provider)
        .bind(&req.token)
        .bind(req.device_name.as_deref())
        .bind(req.platform.as_deref())
        .bind(req.app_version.as_deref())
        .fetch_one(pool)
        .await?;

    Ok(push_device_row_to_response(&row))
}

pub async fn list_push_devices(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<PushDeviceListResponse, NotificationsError> {
    let rows = sqlx::query(LIST_PUSH_DEVICES_SQL)
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    let devices: Vec<PushDeviceResponse> = rows.iter().map(push_device_row_to_response).collect();

    Ok(PushDeviceListResponse { devices })
}

pub async fn update_push_device(
    pool: &PgPool,
    user_id: Uuid,
    device_id: Uuid,
    req: &UpdatePushDeviceRequest,
) -> Result<PushDeviceResponse, NotificationsError> {
    validate_optional_length(
        req.device_name.as_deref(),
        MAX_DEVICE_NAME_LEN,
        "device_name",
    )?;
    validate_optional_length(req.platform.as_deref(), MAX_PLATFORM_LEN, "platform")?;
    validate_optional_length(
        req.app_version.as_deref(),
        MAX_APP_VERSION_LEN,
        "app_version",
    )?;

    let row = sqlx::query(UPDATE_PUSH_DEVICE_SQL)
        .bind(device_id)
        .bind(user_id)
        .bind(req.device_name.as_deref())
        .bind(req.platform.as_deref())
        .bind(req.app_version.as_deref())
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => Ok(push_device_row_to_response(&r)),
        None => {
            // BOLA-safe: covers "doesn't exist", "belongs to another user", and "inactive".
            // The WHERE clause binds user_id AND is_active = true, so no row matches if any
            // of those conditions fail. No follow-up existence probe to avoid leaking state.
            Err(NotificationsError::PushDeviceNotFound)
        }
    }
}

pub async fn delete_push_device(
    pool: &PgPool,
    user_id: Uuid,
    device_id: Uuid,
) -> Result<PushDeviceDeletedResponse, NotificationsError> {
    let result = sqlx::query(DELETE_PUSH_DEVICE_SQL)
        .bind(device_id)
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(NotificationsError::PushDeviceNotFound);
    }
    Ok(PushDeviceDeletedResponse { deleted: true })
}

pub async fn deactivate_stale_devices(
    pool: &PgPool,
    stale_days: i32,
) -> Result<u64, NotificationsError> {
    let clamped = stale_days.clamp(1, 3650);
    let result = sqlx::query(DEACTIVATE_STALE_DEVICES_SQL)
        .bind(clamped)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

fn push_device_row_to_response(r: &sqlx::postgres::PgRow) -> PushDeviceResponse {
    let token: String = r.try_get("token").unwrap_or_default();
    PushDeviceResponse {
        id: r.try_get("id").unwrap_or_default(),
        provider: r.try_get("provider").unwrap_or_default(),
        token_preview: mask_token(&token),
        device_name: r.try_get("device_name").ok().flatten(),
        platform: r.try_get("platform").ok().flatten(),
        app_version: r.try_get("app_version").ok().flatten(),
        last_seen_at: r.try_get("last_seen_at").ok().flatten(),
        is_active: r.try_get("is_active").unwrap_or(false),
        invalidated_at: r.try_get("invalidated_at").ok().flatten(),
        created_at: r.try_get("created_at").unwrap_or_default(),
        updated_at: r.try_get("updated_at").unwrap_or_default(),
    }
}

fn mask_token(token: &str) -> String {
    let chars = token.chars().count();
    if chars <= 12 {
        return "***".to_string();
    }
    let prefix: String = token.chars().take(8).collect();
    let suffix: String = token.chars().skip(chars - 4).collect();
    format!("{prefix}…{suffix}")
}

fn validate_push_provider(provider: &str) -> Result<(), NotificationsError> {
    if !VALID_PUSH_PROVIDERS.contains(&provider) {
        return Err(NotificationsError::InvalidPushProvider(
            provider.to_string(),
        ));
    }
    Ok(())
}

fn validate_push_token(provider: &str, token: &str) -> Result<(), NotificationsError> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return Err(NotificationsError::InvalidPushToken(
            "token must not be empty".to_string(),
        ));
    }
    if trimmed.len() > MAX_PUSH_TOKEN_LEN {
        return Err(NotificationsError::InvalidPushToken(format!(
            "token exceeds maximum length of {MAX_PUSH_TOKEN_LEN} characters"
        )));
    }
    if provider == "unifiedpush" {
        if url::Url::parse(trimmed).is_err() {
            return Err(NotificationsError::InvalidPushToken(
                "unifiedpush token must be a valid URL endpoint".to_string(),
            ));
        }
    } else if !trimmed.bytes().all(|b| b.is_ascii() && (b >= 0x20)) {
        return Err(NotificationsError::InvalidPushToken(
            "token contains invalid characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_optional_length(
    value: Option<&str>,
    max_len: usize,
    field_name: &str,
) -> Result<(), NotificationsError> {
    if let Some(v) = value
        && v.len() > max_len
    {
        return Err(NotificationsError::InvalidPushToken(format!(
            "{field_name} exceeds maximum length of {max_len} characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn parse_cursor_round_trip() {
        let id = Uuid::now_v7();
        let encoded = encode_cursor(id);
        let parsed = parse_cursor(Some(&encoded));
        assert_eq!(parsed, Some(id));
    }

    #[test]
    fn parse_cursor_garbage_returns_none() {
        assert_eq!(parse_cursor(Some("not-base64!")), None);
        assert_eq!(parse_cursor(Some("====")), None);
    }

    #[test]
    fn parse_cursor_missing_id_returns_none() {
        let json = serde_json::json!({ "wrong": "field" });
        let encoded =
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&json).unwrap());
        assert_eq!(parse_cursor(Some(&encoded)), None);
    }

    #[test]
    fn parse_cursor_none_input_returns_none() {
        assert_eq!(parse_cursor(None), None);
    }

    #[test]
    fn validate_category_accepts_known() {
        for c in VALID_CATEGORIES {
            assert!(validate_category(Some(c)).is_ok());
        }
    }

    #[test]
    fn validate_category_rejects_unknown() {
        assert!(matches!(
            validate_category(Some("bogus")),
            Err(NotificationsError::InvalidCategory(_))
        ));
    }

    #[test]
    fn validate_category_accepts_none() {
        assert!(validate_category(None).is_ok());
    }

    #[test]
    fn validate_priority_accepts_known() {
        for p in VALID_PRIORITIES {
            assert!(validate_priority(Some(p)).is_ok());
        }
    }

    #[test]
    fn validate_priority_rejects_unknown() {
        assert!(matches!(
            validate_priority(Some("urgent")),
            Err(NotificationsError::InvalidPriority(_))
        ));
    }

    #[test]
    fn validate_priority_accepts_none() {
        assert!(validate_priority(None).is_ok());
    }

    #[test]
    fn encode_cursor_uses_base64_json() {
        let id = Uuid::now_v7();
        let encoded = encode_cursor(id);
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&encoded)
            .expect("base64 decode");
        let json: serde_json::Value = serde_json::from_slice(&decoded).expect("json parse");
        assert_eq!(json["id"], id.to_string());
    }

    #[test]
    fn validate_push_provider_accepts_known() {
        for p in VALID_PUSH_PROVIDERS {
            assert!(validate_push_provider(p).is_ok());
        }
    }

    #[test]
    fn validate_push_provider_rejects_unknown() {
        assert!(matches!(
            validate_push_provider("windows-push"),
            Err(NotificationsError::InvalidPushProvider(_))
        ));
    }

    #[test]
    fn validate_push_token_rejects_empty() {
        assert!(matches!(
            validate_push_token("fcm", ""),
            Err(NotificationsError::InvalidPushToken(_))
        ));
        assert!(matches!(
            validate_push_token("fcm", "   "),
            Err(NotificationsError::InvalidPushToken(_))
        ));
    }

    #[test]
    fn validate_push_token_rejects_too_long() {
        let huge = "a".repeat(MAX_PUSH_TOKEN_LEN + 1);
        assert!(matches!(
            validate_push_token("fcm", &huge),
            Err(NotificationsError::InvalidPushToken(_))
        ));
    }

    #[test]
    fn validate_push_token_accepts_fcm() {
        let token = "c2aK9KHmw8E:APA91bF7MY9bNnvGAXgbHN58lyDxc9KnuXNXwsqUs4uV4GyeF06HM1hMm-etu63S_4C-GnEtHAxJPJJC4H__VcIk90A69qQz65toFejxyncceg0_j5xwoFWvPQ5pzKo69rUnuCl1GSSv";
        assert!(validate_push_token("fcm", token).is_ok());
    }

    #[test]
    fn validate_push_token_accepts_apns_hex() {
        let token = "00fc13adff785122b4ad28809a3420982341241421348097878e577c991de8f0";
        assert!(validate_push_token("apns", token).is_ok());
    }

    #[test]
    fn validate_push_token_rejects_non_ascii_for_fcm() {
        assert!(matches!(
            validate_push_token("fcm", "héllo🍷"),
            Err(NotificationsError::InvalidPushToken(_))
        ));
    }

    #[test]
    fn validate_push_token_accepts_unifiedpush_url() {
        assert!(
            validate_push_token(
                "unifiedpush",
                "https://ntfy.example.com/duskcue-up/abcdef123"
            )
            .is_ok()
        );
    }

    #[test]
    fn validate_push_token_rejects_unifiedpush_non_url() {
        assert!(matches!(
            validate_push_token("unifiedpush", "not-a-url"),
            Err(NotificationsError::InvalidPushToken(_))
        ));
    }

    #[test]
    fn mask_token_short_returns_mask() {
        assert_eq!(mask_token("abc"), "***");
        assert_eq!(mask_token("123456789012"), "***");
    }

    #[test]
    fn mask_token_long_shows_prefix_suffix() {
        let token = "c2aK9KHmw8E:APA91bF7MY9b";
        let masked = mask_token(token);
        assert!(masked.starts_with("c2aK9KHm"));
        assert!(masked.ends_with("9b"));
        assert!(masked.contains('…'));
    }

    #[test]
    fn validate_optional_length_rejects_overlong() {
        let too_long = "x".repeat(MAX_DEVICE_NAME_LEN + 1);
        assert!(matches!(
            validate_optional_length(Some(&too_long), MAX_DEVICE_NAME_LEN, "device_name"),
            Err(NotificationsError::InvalidPushToken(_))
        ));
    }

    #[test]
    fn validate_optional_length_accepts_none_and_in_bounds() {
        assert!(validate_optional_length(None, MAX_DEVICE_NAME_LEN, "device_name").is_ok());
        let ok = "x".repeat(MAX_DEVICE_NAME_LEN);
        assert!(validate_optional_length(Some(&ok), MAX_DEVICE_NAME_LEN, "device_name").is_ok());
    }
}
