// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use sqlx::Row;
use uuid::Uuid;

use crate::services::i18n::{DEFAULT_LOCALE, REVIEWED_UI_LOCALES, is_reviewed_ui_locale};

use super::error::UsersError;
use super::types::{
    LocaleOptionResponse, UserListResponse, UserPreferencesResponse, UserResponse, UserRow,
    VALID_ROLES, VALID_STATUSES,
};

pub async fn list_users(
    pool: &sqlx::PgPool,
    page: u32,
    page_size: u32,
    status_filter: Option<&str>,
    role_filter: Option<&str>,
) -> Result<UserListResponse, UsersError> {
    let offset = (page - 1) * page_size;

    let count_row = sqlx::query(
        r#"SELECT count(*) as cnt FROM users
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR status = $1)
             AND ($2::text IS NULL OR role = $2)"#,
    )
    .bind(status_filter)
    .bind(role_filter)
    .fetch_one(pool)
    .await?;

    let total: i64 = count_row.get("cnt");

    let rows = sqlx::query(
        r#"SELECT id, username, display_name, email, avatar_url, role, status,
                  has_all_library_access, streaming_policy_id, max_streams,
                  max_transcode_streams, bandwidth_limit_bps,
                  last_login_at, is_active, deleted_at, created_at, updated_at
           FROM users
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR status = $1)
             AND ($2::text IS NULL OR role = $2)
           ORDER BY created_at DESC
           LIMIT $3 OFFSET $4"#,
    )
    .bind(status_filter)
    .bind(role_filter)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<UserResponse> = rows.iter().map(row_to_response).collect();

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (page_size as f64)).ceil() as u32
    };

    Ok(UserListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    })
}

pub async fn get_user(pool: &sqlx::PgPool, user_id: Uuid) -> Result<UserResponse, UsersError> {
    let row = sqlx::query(
        r#"
        SELECT id, username, display_name, email, avatar_url, role, status,
               has_all_library_access, streaming_policy_id, max_streams,
               max_transcode_streams, bandwidth_limit_bps,
               last_login_at, is_active, deleted_at, created_at, updated_at
        FROM users
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(UsersError::NotFound)?;

    Ok(row_to_response(&row))
}

pub async fn get_user_preferences(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<UserPreferencesResponse, UsersError> {
    let row = sqlx::query(
        r#"
        SELECT metadata ->> 'locale' AS locale
        FROM users
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(UsersError::NotFound)?;

    let locale = row
        .try_get::<Option<String>, _>("locale")
        .ok()
        .flatten()
        .filter(|locale| is_reviewed_ui_locale(locale))
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

    Ok(UserPreferencesResponse {
        locale,
        available_locales: reviewed_locale_options(),
    })
}

pub async fn update_user_preferences(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    locale: String,
) -> Result<UserPreferencesResponse, UsersError> {
    let locale = locale.trim();
    if !is_reviewed_ui_locale(locale) {
        return Err(UsersError::InvalidLocale(locale.to_string()));
    }

    let row = sqlx::query(
        r#"
        UPDATE users
        SET metadata = jsonb_set(
                COALESCE(metadata, '{}'::jsonb),
                '{locale}',
                to_jsonb($2::text),
                true
            ),
            updated_at = now()
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING metadata ->> 'locale' AS locale
        "#,
    )
    .bind(user_id)
    .bind(locale)
    .fetch_optional(pool)
    .await?
    .ok_or(UsersError::NotFound)?;

    let locale = row
        .try_get::<Option<String>, _>("locale")
        .ok()
        .flatten()
        .unwrap_or_else(|| DEFAULT_LOCALE.to_string());

    Ok(UserPreferencesResponse {
        locale,
        available_locales: reviewed_locale_options(),
    })
}

fn reviewed_locale_options() -> Vec<LocaleOptionResponse> {
    REVIEWED_UI_LOCALES
        .iter()
        .map(|locale| {
            let tag = locale.to_string();
            LocaleOptionResponse {
                name: locale_display_name(&tag),
                text_direction: locale_text_direction(&tag).to_string(),
                is_default: *locale == DEFAULT_LOCALE,
                tag,
            }
        })
        .collect()
}

fn locale_display_name(tag: &str) -> String {
    match tag {
        "en" => "English".to_string(),
        "fr" => "French".to_string(),
        "de" => "Deutsch".to_string(),
        "es" => "Spanish".to_string(),
        "it" => "Italiano".to_string(),
        "ar" => "Arabic".to_string(),
        "zh-Hans" => "Simplified Chinese".to_string(),
        "zh-Hant" => "Traditional Chinese".to_string(),
        _ => tag.to_string(),
    }
}

fn locale_text_direction(tag: &str) -> &'static str {
    match tag {
        "ar" => "rtl",
        _ => "ltr",
    }
}

pub struct UpdateUserParams {
    pub user_id: Uuid,
    pub admin_user_id: Uuid,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    pub has_all_library_access: Option<bool>,
    pub streaming_policy_id: Option<Uuid>,
    pub max_streams: Option<i32>,
    pub max_transcode_streams: Option<i32>,
    pub bandwidth_limit_bps: Option<i64>,
}

pub async fn update_user(
    pool: &sqlx::PgPool,
    params: UpdateUserParams,
) -> Result<UserResponse, UsersError> {
    let existing =
        sqlx::query("SELECT id, role, username FROM users WHERE id = $1 AND deleted_at IS NULL")
            .bind(params.user_id)
            .fetch_optional(pool)
            .await?
            .ok_or(UsersError::NotFound)?;

    let existing_role: String = existing.get("role");

    if existing_role == "owner" {
        return Err(UsersError::OwnerImmutable);
    }

    if params.user_id == params.admin_user_id && (params.role.is_some() || params.status.is_some())
    {
        return Err(UsersError::CannotModifySelf);
    }

    if let Some(ref r) = params.role
        && !VALID_ROLES.contains(&r.as_str())
    {
        return Err(UsersError::InvalidRole(r.clone()));
    }

    if let Some(ref s) = params.status
        && !VALID_STATUSES.contains(&s.as_str())
    {
        return Err(UsersError::InvalidStatus(s.clone()));
    }

    if let Some(ref e) = params.email {
        let existing_email = sqlx::query(
            "SELECT id FROM users WHERE email = $1 AND id != $2 AND deleted_at IS NULL",
        )
        .bind(e)
        .bind(params.user_id)
        .fetch_optional(pool)
        .await?;

        if existing_email.is_some() {
            return Err(UsersError::EmailTaken);
        }
    }

    let has_changes = params.display_name.is_some()
        || params.email.is_some()
        || params.avatar_url.is_some()
        || params.role.is_some()
        || params.status.is_some()
        || params.has_all_library_access.is_some()
        || params.streaming_policy_id.is_some()
        || params.max_streams.is_some()
        || params.max_transcode_streams.is_some()
        || params.bandwidth_limit_bps.is_some();

    if !has_changes {
        return get_user(pool, params.user_id).await;
    }

    let row = sqlx::query(
        r#"
        UPDATE users SET
            display_name = COALESCE($2, display_name),
            email = COALESCE($3, email),
            avatar_url = CASE WHEN $4::boolean THEN $5 ELSE avatar_url END,
            role = COALESCE($6, role),
            status = COALESCE($7, status),
            has_all_library_access = COALESCE($8, has_all_library_access),
            streaming_policy_id = COALESCE($9, streaming_policy_id),
            max_streams = COALESCE($10, max_streams),
            max_transcode_streams = COALESCE($11, max_transcode_streams),
            bandwidth_limit_bps = COALESCE($12, bandwidth_limit_bps),
            updated_at = now()
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING id, username, display_name, email, avatar_url, role, status,
                  has_all_library_access, streaming_policy_id, max_streams,
                  max_transcode_streams, bandwidth_limit_bps,
                  last_login_at, is_active, deleted_at, created_at, updated_at
        "#,
    )
    .bind(params.user_id)
    .bind(&params.display_name)
    .bind(&params.email)
    .bind(params.avatar_url.is_some())
    .bind(&params.avatar_url)
    .bind(&params.role)
    .bind(&params.status)
    .bind(params.has_all_library_access)
    .bind(params.streaming_policy_id)
    .bind(params.max_streams)
    .bind(params.max_transcode_streams)
    .bind(params.bandwidth_limit_bps)
    .fetch_optional(pool)
    .await?
    .ok_or(UsersError::NotFound)?;

    Ok(row_to_response(&row))
}

pub async fn soft_delete_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    admin_user_id: Uuid,
) -> Result<(), UsersError> {
    let existing = sqlx::query("SELECT id, role FROM users WHERE id = $1 AND deleted_at IS NULL")
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(UsersError::NotFound)?;

    let role: String = existing.get("role");

    if role == "owner" {
        return Err(UsersError::OwnerCannotBeDeleted);
    }

    if user_id == admin_user_id {
        return Err(UsersError::CannotModifySelf);
    }

    sqlx::query(
        "UPDATE users SET deleted_at = now(), updated_at = now(), is_active = false WHERE id = $1",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    let _ = sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;

    let _ = sqlx::query("DELETE FROM profile_device_preferences WHERE owner_user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await;

    Ok(())
}

pub fn row_to_user_row(row: &sqlx::postgres::PgRow) -> UserRow {
    UserRow {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        email: row.try_get("email").ok(),
        avatar_url: row.try_get("avatar_url").ok(),
        role: row.get("role"),
        status: row.get("status"),
        has_all_library_access: row.get("has_all_library_access"),
        streaming_policy_id: row.try_get("streaming_policy_id").ok(),
        max_streams: row.try_get("max_streams").ok(),
        max_transcode_streams: row.try_get("max_transcode_streams").ok(),
        bandwidth_limit_bps: row.try_get("bandwidth_limit_bps").ok(),
        last_login_at: row.try_get("last_login_at").ok(),
        is_active: row.get("is_active"),
        deleted_at: row.try_get("deleted_at").ok(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_response(row: &sqlx::postgres::PgRow) -> UserResponse {
    let user = row_to_user_row(row);
    UserResponse {
        id: user.id,
        username: user.username,
        display_name: user.display_name,
        email: user.email,
        avatar_url: user.avatar_url,
        role: user.role,
        status: user.status,
        has_all_library_access: user.has_all_library_access,
        streaming_policy_id: user.streaming_policy_id,
        max_streams: user.max_streams,
        max_transcode_streams: user.max_transcode_streams,
        bandwidth_limit_bps: user.bandwidth_limit_bps,
        last_login_at: user.last_login_at,
        is_active: user.is_active,
        created_at: user.created_at,
        updated_at: user.updated_at,
    }
}

pub async fn validate_streaming_policy_exists(
    pool: &sqlx::PgPool,
    policy_id: Uuid,
) -> Result<bool, UsersError> {
    let row = sqlx::query("SELECT id FROM streaming_policies WHERE id = $1")
        .bind(policy_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.is_some())
}
