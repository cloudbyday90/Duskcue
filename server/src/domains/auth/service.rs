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

use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use rand::Rng;
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

use super::error::AuthError;
use super::types::*;

pub async fn validate_session(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<ValidatedSession, AuthError> {
    let token_hash = sha256_hex(token);

    let row = sqlx::query(
        r#"
        SELECT id, user_id, token_hash, device_id, device_name,
            client_name, client_version, client_platform,
            ip_address::text as ip_address, user_agent, is_secure,
            expires_at, last_active_at
        FROM user_sessions
        WHERE token_hash = $1 AND expires_at > now()
        "#,
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::SessionExpired)?;

    let session = row_to_session(&row);

    let user = sqlx::query(
        r#"
        SELECT id, username, display_name, role, has_all_library_access
        FROM users
        WHERE id = $1 AND deleted_at IS NULL AND status = 'active'
        "#,
    )
    .bind(session.user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::SessionExpired)?;

    let user_id: Uuid = user.get("id");
    let username: String = user.get("username");
    let display_name: String = user.get("display_name");
    let role: String = user.get("role");
    let has_all_library_access: bool = user.get("has_all_library_access");

    let capabilities = resolve_capabilities(pool, user_id, &role).await?;

    Ok(ValidatedSession {
        session,
        user_id,
        username,
        display_name,
        role,
        capabilities,
        has_all_library_access,
    })
}

pub async fn resolve_capabilities(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    role: &str,
) -> Result<Vec<String>, AuthError> {
    if role == "owner" {
        return Ok(ALL_CAPABILITIES.iter().map(|s| s.to_string()).collect());
    }

    let rows = sqlx::query(
        "SELECT capability, is_granted FROM user_capabilities WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(role_default_capabilities(role));
    }

    let mut caps = role_default_capabilities(role);
    for row in &rows {
        let cap_name: String = row.get("capability");
        let granted: bool = row.get("is_granted");
        if granted && !caps.contains(&cap_name) {
            caps.push(cap_name);
        } else if !granted {
            caps.retain(|c| c != &cap_name);
        }
    }

    Ok(caps)
}

pub async fn create_session(
    pool: &sqlx::PgPool,
    state: &AppState,
    user_id: Uuid,
    device_info: &DeviceInfo,
) -> Result<(String, UserSession), AuthError> {
    let token = generate_session_token();
    let token_hash = sha256_hex(&token);

    let config = state.runtime_config.load();
    let absolute_timeout_days = config.auth.session_absolute_timeout_days;
    drop(config);

    let row = sqlx::query(
        r#"
        INSERT INTO user_sessions (
            user_id, token_hash, device_id, device_name,
            client_name, client_version, client_platform,
            ip_address, user_agent, is_secure, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8::inet, $9, $10, now() + ($11 || ' days')::interval)
        RETURNING id, expires_at, last_active_at
        "#,
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(&device_info.device_id)
    .bind(&device_info.device_name)
    .bind(&device_info.client_name)
    .bind(&device_info.client_version)
    .bind(&device_info.client_platform)
    .bind(&device_info.ip_address)
    .bind(&device_info.user_agent)
    .bind(device_info.is_secure)
    .bind(format!("{}", absolute_timeout_days))
    .fetch_one(pool)
    .await?;

    let session_id: Uuid = row.get("id");
    let expires_at: DateTime<Utc> = row.get("expires_at");
    let last_active_at: DateTime<Utc> = row.get("last_active_at");

    let session = UserSession {
        id: session_id,
        user_id,
        token_hash,
        device_id: device_info.device_id.clone(),
        device_name: device_info.device_name.clone(),
        client_name: device_info.client_name.clone(),
        client_version: device_info.client_version.clone(),
        client_platform: device_info.client_platform.clone(),
        ip_address: device_info.ip_address.clone(),
        user_agent: device_info.user_agent.clone(),
        is_secure: device_info.is_secure,
        expires_at,
        last_active_at,
    };

    Ok((token, session))
}

pub async fn setup_owner(
    pool: &sqlx::PgPool,
    username: String,
    display_name: String,
    password: Option<String>,
) -> Result<(Uuid, String), AuthError> {
    let password_hash = match password {
        Some(pw) => Some(hash_password(&pw)?),
        None => None,
    };

    let row = sqlx::query(
        r#"
        INSERT INTO users (username, display_name, password_hash, role, status, has_all_library_access)
        VALUES ($1, $2, $3, 'owner', 'active', true)
        RETURNING id
        "#,
    )
    .bind(&username)
    .bind(&display_name)
    .bind(&password_hash)
    .fetch_one(pool)
    .await?;

    let user_id: Uuid = row.get("id");

    sqlx::query(
        r#"UPDATE server_config SET auth = jsonb_set(auth, '{setup_complete}', 'true')"#,
    )
    .execute(pool)
    .await?;

    let token = generate_session_token();
    let token_hash = sha256_hex(&token);

    sqlx::query(
        r#"INSERT INTO user_sessions (user_id, token_hash, is_secure, expires_at) VALUES ($1, $2, false, now() + '90 days'::interval)"#,
    )
    .bind(user_id)
    .bind(&token_hash)
    .execute(pool)
    .await?;

    Ok((user_id, token))
}

pub async fn is_setup_complete(pool: &sqlx::PgPool) -> Result<bool, AuthError> {
    let row = sqlx::query(
        r#"SELECT COALESCE((auth->>'setup_complete')::boolean, false) as setup_complete FROM server_config LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;

    Ok(row
        .and_then(|r| r.try_get::<bool, _>("setup_complete").ok())
        .unwrap_or(false))
}

pub async fn revoke_session(
    pool: &sqlx::PgPool,
    session_id: Uuid,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let result = sqlx::query(
        "DELETE FROM user_sessions WHERE id = $1 AND user_id = $2",
    )
    .bind(session_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::SessionExpired);
    }

    Ok(())
}

pub async fn revoke_all_sessions(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<u64, AuthError> {
    let result = sqlx::query(
        "DELETE FROM user_sessions WHERE user_id = $1",
    )
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn list_user_sessions(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<UserSession>, AuthError> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, token_hash, device_id, device_name,
            client_name, client_version, client_platform,
            ip_address::text as ip_address, user_agent, is_secure,
            expires_at, last_active_at
        FROM user_sessions
        WHERE user_id = $1 AND expires_at > now()
        ORDER BY last_active_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(row_to_session).collect())
}

pub async fn authenticate_invite_code(
    pool: &sqlx::PgPool,
    state: &AppState,
    code: &str,
    device_info: &DeviceInfo,
) -> Result<(Uuid, String, UserSummary), AuthError> {
    let code_hash = sha256_hex(code);

    let invitation = sqlx::query(
        r#"
        SELECT id, user_id, code_hash, code_prefix, email, display_name, role,
               capabilities, library_ids, has_all_library_access, streaming_policy_id,
               max_uses, use_count, expires_at, is_revoked
        FROM invitations
        WHERE code_hash = $1
        "#,
    )
    .bind(&code_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::InviteCodeInvalid)?;

    let invitation_id: Uuid = invitation.get("id");
    let inv_user_id: Option<Uuid> = invitation.try_get("user_id").ok();
    let is_revoked: bool = invitation.get("is_revoked");
    let expires_at: Option<DateTime<Utc>> = invitation.try_get("expires_at").ok();
    let max_uses: i32 = invitation.get("max_uses");
    let use_count: i32 = invitation.get("use_count");
    let inv_email: String = invitation.get("email");
    let inv_display_name: Option<String> = invitation.try_get("display_name").ok();
    let inv_role: String = invitation.get("role");
    let inv_has_all_library_access: bool = invitation.get("has_all_library_access");
    let inv_capabilities: serde_json::Value = invitation.get("capabilities");

    if is_revoked {
        return Err(AuthError::InviteCodeRevoked);
    }

    if let Some(expires) = expires_at
        && expires < chrono::Utc::now()
    {
        return Err(AuthError::InviteCodeInvalid);
    }

    if use_count >= max_uses {
        return Err(AuthError::InviteCodeUseLimitExceeded);
    }

    let user_id = match inv_user_id {
        Some(existing_user_id) => existing_user_id,
        None => {
            let caps: Vec<String> = serde_json::from_value(inv_capabilities).unwrap_or_default();

            let display = inv_display_name.unwrap_or_else(|| "New User".to_string());

            let row = sqlx::query(
                r#"
                INSERT INTO users (username, display_name, email, role, status, has_all_library_access)
                VALUES ($1, $2, $3, $4, 'active', $5)
                RETURNING id
                "#,
            )
            .bind(format!("user_{}", Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).as_simple()))
            .bind(&display)
            .bind(&inv_email)
            .bind(&inv_role)
            .bind(inv_has_all_library_access)
            .fetch_one(pool)
            .await?;

            let new_user_id: Uuid = row.get("id");

            for cap in &caps {
                sqlx::query(
                    "INSERT INTO user_capabilities (user_id, capability, is_granted) VALUES ($1, $2, true) ON CONFLICT (user_id, capability) DO NOTHING",
                )
                .bind(new_user_id)
                .bind(cap)
                .execute(pool)
                .await?;
            }

            sqlx::query(
                "UPDATE invitations SET user_id = $1 WHERE id = $2",
            )
            .bind(new_user_id)
            .bind(invitation_id)
            .execute(pool)
            .await?;

            new_user_id
        }
    };

    sqlx::query(
        "UPDATE invitations SET use_count = use_count + 1 WHERE id = $1",
    )
    .bind(invitation_id)
    .execute(pool)
    .await?;

    let (token, _session) = create_session(pool, state, user_id, device_info).await?;

    let user = sqlx::query(
        r#"SELECT id, username, display_name, role, has_all_library_access FROM users WHERE id = $1"#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let db_user_id: Uuid = user.get("id");
    let db_username: String = user.get("username");
    let db_display_name: String = user.get("display_name");
    let db_role: String = user.get("role");
    let db_has_all: bool = user.get("has_all_library_access");

    let capabilities = resolve_capabilities(pool, db_user_id, &db_role).await?;

    let summary = UserSummary {
        id: db_user_id,
        username: db_username,
        display_name: db_display_name,
        role: db_role,
        capabilities,
        has_all_library_access: db_has_all,
    };

    Ok((user_id, token, summary))
}

pub async fn get_user_for_login(
    pool: &sqlx::PgPool,
    username: &str,
) -> Result<Option<LoginUser>, AuthError> {
    let row = sqlx::query(
        r#"
        SELECT id, username, display_name, role, password_hash, status,
               failed_login_attempts, locked_until, has_all_library_access
        FROM users
        WHERE username = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    let row = match row {
        Some(r) => r,
        None => return Ok(None),
    };

    Ok(Some(LoginUser {
        id: row.get("id"),
        username: row.get("username"),
        display_name: row.get("display_name"),
        role: row.get("role"),
        password_hash: row.try_get("password_hash").ok(),
        status: row.get("status"),
        failed_login_attempts: row.get("failed_login_attempts"),
        locked_until: row.try_get("locked_until").ok(),
        has_all_library_access: row.get("has_all_library_access"),
    }))
}

pub async fn reset_login_failures(pool: &sqlx::PgPool, user_id: Uuid) -> Result<(), AuthError> {
    sqlx::query(
        "UPDATE users SET failed_login_attempts = 0, locked_until = NULL, last_login_at = now() WHERE id = $1",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_count(pool: &sqlx::PgPool) -> Result<i64, AuthError> {
    let row = sqlx::query("SELECT count(*) as cnt FROM users")
        .fetch_one(pool)
        .await?;
    Ok(row.get("cnt"))
}

fn row_to_session(row: &sqlx::postgres::PgRow) -> UserSession {
    UserSession {
        id: row.get("id"),
        user_id: row.get("user_id"),
        token_hash: row.get("token_hash"),
        device_id: row.try_get("device_id").ok(),
        device_name: row.try_get("device_name").ok(),
        client_name: row.try_get("client_name").ok(),
        client_version: row.try_get("client_version").ok(),
        client_platform: row.try_get("client_platform").ok(),
        ip_address: row.try_get("ip_address").ok(),
        user_agent: row.try_get("user_agent").ok(),
        is_secure: row.get("is_secure"),
        expires_at: row.get("expires_at"),
        last_active_at: row.get("last_active_at"),
    }
}

pub fn generate_session_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex_encode(&bytes)
}

fn sha256_hex(input: &str) -> String {
    use ring::digest::{digest, SHA256};
    let result = digest(&SHA256, input.as_bytes());
    hex_encode(result.as_ref())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let algorithm = ring::pbkdf2::PBKDF2_HMAC_SHA256;
    let mut rng = rand::rng();
    let mut salt_bytes = [0u8; 16];
    rng.fill(&mut salt_bytes);

    let mut hash = [0u8; 32];
    ring::pbkdf2::derive(
        algorithm,
        NonZeroU32::new(600_000).unwrap(),
        &salt_bytes,
        password.as_bytes(),
        &mut hash,
    );

    Ok(format!(
        "pbkdf2:{}:{}",
        hex_encode(&salt_bytes),
        hex_encode(&hash)
    ))
}

pub fn verify_password_hash(hash: &str, password: &str) -> Result<(), AuthError> {
    let parts: Vec<&str> = hash.splitn(3, ':').collect();
    if parts.len() != 3 || parts[0] != "pbkdf2" {
        return Err(AuthError::InvalidCredentials);
    }

    let salt = hex_decode(parts[1]).map_err(|_| AuthError::InvalidCredentials)?;
    let stored_hash = hex_decode(parts[2]).map_err(|_| AuthError::InvalidCredentials)?;

    ring::pbkdf2::verify(
        ring::pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600_000).unwrap(),
        &salt,
        password.as_bytes(),
        &stored_hash,
    )
    .map_err(|_| AuthError::InvalidCredentials)?;

    Ok(())
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if !hex.len().is_multiple_of(2) {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

fn role_default_capabilities(role: &str) -> Vec<String> {
    match role {
        "admin" => vec![
            "play_media", "can_transcode", "can_download", "can_delete_media",
            "can_manage_libraries", "can_manage_users", "can_view_analytics",
            "can_manage_server", "can_manage_scheduled_tasks", "can_share_content",
            "can_remote_control",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        "member" => vec!["play_media", "can_download", "can_share_content"]
            .into_iter()
            .map(String::from)
            .collect(),
        "guest" => vec!["play_media".to_string()],
        _ => vec![],
    }
}

pub struct DeviceInfo {
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub client_name: Option<String>,
    pub client_version: Option<String>,
    pub client_platform: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub is_secure: bool,
}

pub static ALL_CAPABILITIES: &[&str] = &[
    "play_media",
    "can_transcode",
    "can_download",
    "can_delete_media",
    "can_manage_libraries",
    "can_manage_users",
    "can_view_analytics",
    "can_manage_server",
    "can_manage_scheduled_tasks",
    "can_use_live_tv",
    "can_share_content",
    "can_remote_control",
];
