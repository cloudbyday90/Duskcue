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

const DELETE_NOTIFICATION_SQL: &str =
    "DELETE FROM notifications WHERE id = $1 AND user_id = $2";

const DELETE_READ_SQL: &str =
    "DELETE FROM notifications WHERE user_id = $1 AND is_read = true";

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
    let rows = if has_more { &rows[..limit as usize] } else { &rows };

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
    Ok(UnreadCountResponse { unread_count: count })
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
            let exists: Option<Uuid> = sqlx::query_scalar(
                "SELECT id FROM notifications WHERE id = $1 AND user_id = $2",
            )
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
        notification_type: r
            .try_get("notification_type_name")
            .unwrap_or_default(),
        category: r
            .try_get("notification_type_category")
            .unwrap_or_default(),
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
    base64::engine::general_purpose::STANDARD
        .encode(serde_json::to_vec(&json).unwrap_or_default())
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
        let encoded = base64::engine::general_purpose::STANDARD
            .encode(serde_json::to_vec(&json).unwrap());
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
        let decoded =
            base64::engine::general_purpose::STANDARD.decode(&encoded).expect("base64 decode");
        let json: serde_json::Value =
            serde_json::from_slice(&decoded).expect("json parse");
        assert_eq!(json["id"], id.to_string());
    }
}
