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
use dashmap::DashMap;
use rand::Rng;
use sqlx::Row;
use uuid::Uuid;
use webauthn_rs::prelude::*;

use crate::state::{AppState, WebauthnChallenge};

use super::error::AuthError;
use super::types::*;

pub async fn validate_session(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<ValidatedSession, AuthError> {
    let token_hash = sha256_hex(token);

    let row = sqlx::query(
        r#"
        SELECT id, user_id, active_profile_id, token_hash, device_id, device_name,
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
    let active_profile_id = session.active_profile_id;

    Ok(ValidatedSession {
        session,
        user_id,
        username,
        display_name,
        role,
        capabilities,
        has_all_library_access,
        active_profile_id,
    })
}

pub fn is_idle_expired(session: &UserSession, idle_timeout_hours: Option<i32>) -> bool {
    let Some(hours) = idle_timeout_hours else {
        return false;
    };

    if hours <= 0 {
        return false;
    }

    let now = chrono::Utc::now();
    let idle_duration = now - session.last_active_at;
    idle_duration.num_hours() >= hours as i64
}

pub async fn resolve_capabilities(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    role: &str,
) -> Result<Vec<String>, AuthError> {
    if role == "owner" {
        return Ok(ALL_CAPABILITIES.iter().map(|s| s.to_string()).collect());
    }

    let rows =
        sqlx::query("SELECT capability, is_granted FROM user_capabilities WHERE user_id = $1")
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
    let active_profile_id = ensure_default_profile(pool, user_id).await?;
    let token = generate_session_token();
    let token_hash = sha256_hex(&token);

    let config = state.runtime_config.load();
    let absolute_timeout_days = config.auth.session_absolute_timeout_days;
    drop(config);

    let row = sqlx::query(
        r#"
        INSERT INTO user_sessions (
            user_id, active_profile_id, token_hash, device_id, device_name,
            client_name, client_version, client_platform,
            ip_address, user_agent, is_secure, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::inet, $10, $11, now() + ($12 || ' days')::interval)
        RETURNING id, expires_at, last_active_at
        "#,
    )
    .bind(user_id)
    .bind(active_profile_id)
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
        active_profile_id,
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
) -> Result<(Uuid, String, Uuid), AuthError> {
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
    let active_profile_id = ensure_default_profile(pool, user_id).await?;

    sqlx::query(r#"UPDATE server_config SET auth = jsonb_set(auth, '{setup_complete}', 'true')"#)
        .execute(pool)
        .await?;

    let token = generate_session_token();
    let token_hash = sha256_hex(&token);

    sqlx::query(
        r#"INSERT INTO user_sessions (user_id, active_profile_id, token_hash, is_secure, expires_at) VALUES ($1, $2, $3, false, now() + '90 days'::interval)"#,
    )
    .bind(user_id)
    .bind(active_profile_id)
    .bind(&token_hash)
    .execute(pool)
    .await?;

    Ok((user_id, token, active_profile_id))
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
    let result = sqlx::query("DELETE FROM user_sessions WHERE id = $1 AND user_id = $2")
        .bind(session_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::SessionExpired);
    }

    Ok(())
}

pub async fn revoke_all_sessions(pool: &sqlx::PgPool, user_id: Uuid) -> Result<u64, AuthError> {
    let result = sqlx::query("DELETE FROM user_sessions WHERE user_id = $1")
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
        SELECT id, user_id, active_profile_id, token_hash, device_id, device_name,
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

            sqlx::query("UPDATE invitations SET user_id = $1 WHERE id = $2")
                .bind(new_user_id)
                .bind(invitation_id)
                .execute(pool)
                .await?;

            new_user_id
        }
    };

    sqlx::query("UPDATE invitations SET use_count = use_count + 1 WHERE id = $1")
        .bind(invitation_id)
        .execute(pool)
        .await?;

    let (token, session) = create_session(pool, state, user_id, device_info).await?;

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
        active_profile_id: session.active_profile_id,
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
        active_profile_id: row.get("active_profile_id"),
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

async fn ensure_default_profile(pool: &sqlx::PgPool, user_id: Uuid) -> Result<Uuid, AuthError> {
    if let Some(profile_id) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM user_profiles WHERE owner_user_id = $1 AND is_default = true",
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(profile_id);
    }

    let display_name: String = sqlx::query_scalar("SELECT display_name FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    let profile_id: Uuid = sqlx::query_scalar(
        "INSERT INTO user_profiles (owner_user_id, name, profile_type, is_default) VALUES ($1, $2, 'standard', true) RETURNING id",
    )
    .bind(user_id)
    .bind(display_name)
    .fetch_one(pool)
    .await?;
    Ok(profile_id)
}

pub fn generate_session_token() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex_encode(&bytes)
}

fn sha256_hex(input: &str) -> String {
    use ring::digest::{SHA256, digest};
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
            "play_media",
            "can_transcode",
            "can_download",
            "can_delete_media",
            "can_manage_libraries",
            "can_manage_users",
            "can_view_analytics",
            "can_manage_server",
            "can_manage_scheduled_tasks",
            "can_share_content",
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

pub async fn start_passkey_registration(
    state: &AppState,
    user_id: Uuid,
    username: &str,
    display_name: &str,
    _passkey_name: &str,
) -> Result<(String, serde_json::Value), AuthError> {
    let existing_passkeys = load_user_passkey_credentials(&state.pool, user_id).await?;

    let exclude_credentials: Vec<CredentialID> = existing_passkeys
        .iter()
        .map(|pk| pk.credential_id.clone())
        .collect();

    let (ccr, registration_state) = state
        .webauthn
        .start_passkey_registration(
            user_id,
            username,
            display_name,
            if exclude_credentials.is_empty() {
                None
            } else {
                Some(exclude_credentials)
            },
        )
        .map_err(|e| AuthError::WebauthnRegistrationFailed {
            reason: e.to_string(),
        })?;

    let challenge_id = generate_challenge_id();
    let creation_options =
        serde_json::to_value(&ccr).map_err(|e| AuthError::WebauthnRegistrationFailed {
            reason: format!("Failed to serialize creation options: {}", e),
        })?;

    state.webauthn_challenges.insert(
        challenge_id.clone(),
        WebauthnChallenge {
            registration_state: Some(registration_state),
            authentication_state: None,
            user_id: Some(user_id),
            created_at: std::time::Instant::now(),
        },
    );

    Ok((challenge_id, creation_options))
}

pub async fn finish_passkey_registration(
    state: &AppState,
    challenge_id: &str,
    credential_json: &serde_json::Value,
    passkey_name: &str,
) -> Result<PasskeyResponse, AuthError> {
    let (_, challenge) = state
        .webauthn_challenges
        .remove(challenge_id)
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    let registration_state = challenge
        .registration_state
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    if challenge.created_at.elapsed() > std::time::Duration::from_secs(300) {
        return Err(AuthError::WebauthnChallengeExpired);
    }

    let user_id = challenge
        .user_id
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    let registration = serde_json::from_value::<RegisterPublicKeyCredential>(
        credential_json.clone(),
    )
    .map_err(|e| AuthError::WebauthnRegistrationFailed {
        reason: format!("Invalid credential data: {}", e),
    })?;

    let passkey = state
        .webauthn
        .finish_passkey_registration(&registration, &registration_state)
        .map_err(|e| AuthError::WebauthnRegistrationFailed {
            reason: e.to_string(),
        })?;

    let credential_id_bytes = passkey.cred_id().clone();
    let public_key_bytes =
        serde_json::to_vec(&passkey).map_err(|e| AuthError::WebauthnRegistrationFailed {
            reason: format!("Failed to serialize passkey: {}", e),
        })?;

    let credential: webauthn_rs::prelude::Credential = passkey.clone().into();
    let counter_val: u32 = credential.counter;

    let transports_list: Vec<String> = credential
        .transports
        .unwrap_or_default()
        .iter()
        .map(|t| format!("{:?}", t).to_lowercase())
        .collect();
    let transports_json =
        serde_json::to_value(&transports_list).unwrap_or(serde_json::Value::Array(vec![]));

    let row = sqlx::query(
        r#"
        INSERT INTO user_passkeys (user_id, credential_id, public_key, sign_count, transports, attestation_type, aaguid, name)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, created_at
        "#,
    )
    .bind(user_id)
    .bind(&credential_id_bytes)
    .bind(&public_key_bytes)
    .bind(counter_val as i64)
    .bind(&transports_json)
    .bind("none")
    .bind(credential_id_bytes.get(..16).map(|b| {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(b);
        Uuid::from_bytes(bytes)
    }))
    .bind(passkey_name)
    .fetch_one(&state.pool)
    .await?;

    let passkey_id: Uuid = row.get("id");
    let created_at: DateTime<Utc> = row.get("created_at");

    Ok(PasskeyResponse {
        id: passkey_id,
        name: passkey_name.to_string(),
        aaguid: None,
        transports: transports_list,
        last_used_at: None,
        created_at,
    })
}

pub async fn start_passkey_authentication(
    state: &AppState,
) -> Result<(String, serde_json::Value), AuthError> {
    let (request_options, auth_state) =
        state
            .webauthn
            .start_passkey_authentication(&[])
            .map_err(|e| AuthError::WebauthnAuthenticationFailed {
                reason: e.to_string(),
            })?;

    let challenge_id = generate_challenge_id();
    let options_value = serde_json::to_value(&request_options).map_err(|e| {
        AuthError::WebauthnAuthenticationFailed {
            reason: format!("Failed to serialize request options: {}", e),
        }
    })?;

    state.webauthn_challenges.insert(
        challenge_id.clone(),
        WebauthnChallenge {
            registration_state: None,
            authentication_state: Some(auth_state),
            user_id: None,
            created_at: std::time::Instant::now(),
        },
    );

    Ok((challenge_id, options_value))
}

pub async fn finish_passkey_authentication(
    state: &AppState,
    challenge_id: &str,
    credential_json: &serde_json::Value,
    device_info: &DeviceInfo,
) -> Result<(String, UserSummary), AuthError> {
    let (_, challenge) = state
        .webauthn_challenges
        .remove(challenge_id)
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    let authentication_state = challenge
        .authentication_state
        .ok_or(AuthError::WebauthnChallengeExpired)?;

    if challenge.created_at.elapsed() > std::time::Duration::from_secs(300) {
        return Err(AuthError::WebauthnChallengeExpired);
    }

    let assertion = serde_json::from_value::<PublicKeyCredential>(credential_json.clone())
        .map_err(|e| AuthError::WebauthnAuthenticationFailed {
            reason: format!("Invalid assertion data: {}", e),
        })?;

    let auth_result = state
        .webauthn
        .finish_passkey_authentication(&assertion, &authentication_state)
        .map_err(|e| AuthError::WebauthnAuthenticationFailed {
            reason: e.to_string(),
        })?;

    let credential_id = auth_result.cred_id().clone();

    let row = sqlx::query(
        r#"
        SELECT up.id, up.user_id, up.name, up.sign_count, up.public_key, up.aaguid,
               up.transports, up.last_used_at, up.created_at,
               u.username, u.display_name, u.role, u.has_all_library_access, u.status
        FROM user_passkeys up
        JOIN users u ON u.id = up.user_id AND u.deleted_at IS NULL AND u.status = 'active'
        WHERE up.credential_id = $1
        "#,
    )
    .bind(&credential_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AuthError::PasskeyNotFound)?;

    let user_id: Uuid = row.get("user_id");
    let username: String = row.get("username");
    let display_name: String = row.get("display_name");
    let role: String = row.get("role");
    let has_all_library_access: bool = row.get("has_all_library_access");

    let new_counter = auth_result.counter();
    sqlx::query(
        "UPDATE user_passkeys SET sign_count = $1, last_used_at = now() WHERE credential_id = $2",
    )
    .bind(new_counter as i64)
    .bind(&credential_id)
    .execute(&state.pool)
    .await?;

    let (token, session) = create_session(&state.pool, state, user_id, device_info).await?;

    let capabilities = resolve_capabilities(&state.pool, user_id, &role).await?;

    let summary = UserSummary {
        id: user_id,
        username,
        display_name,
        role,
        capabilities,
        has_all_library_access,
        active_profile_id: session.active_profile_id,
    };

    Ok((token, summary))
}

pub async fn list_user_passkeys(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<PasskeyResponse>, AuthError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, aaguid, transports, last_used_at, created_at
        FROM user_passkeys
        WHERE user_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| PasskeyResponse {
            id: row.get("id"),
            name: row.get("name"),
            aaguid: row.try_get("aaguid").ok(),
            transports: serde_json::from_value(row.get::<serde_json::Value, _>("transports"))
                .unwrap_or_default(),
            last_used_at: row.try_get("last_used_at").ok(),
            created_at: row.get("created_at"),
        })
        .collect())
}

pub async fn delete_passkey(
    pool: &sqlx::PgPool,
    passkey_id: Uuid,
    user_id: Uuid,
) -> Result<(), AuthError> {
    let result = sqlx::query("DELETE FROM user_passkeys WHERE id = $1 AND user_id = $2")
        .bind(passkey_id)
        .bind(user_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AuthError::PasskeyNotFound);
    }

    Ok(())
}

struct PasskeyCredentialRow {
    credential_id: CredentialID,
}

async fn load_user_passkey_credentials(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<PasskeyCredentialRow>, AuthError> {
    let rows = sqlx::query("SELECT credential_id FROM user_passkeys WHERE user_id = $1")
        .bind(user_id)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| {
            let bytes: Vec<u8> = row.get("credential_id");
            PasskeyCredentialRow {
                credential_id: bytes,
            }
        })
        .collect())
}

fn generate_challenge_id() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 32] = rng.random();
    hex_encode(&bytes)
}

pub fn expire_challenges(challenges: &DashMap<String, WebauthnChallenge>) {
    let expired_keys: Vec<String> = challenges
        .iter()
        .filter(|entry| entry.created_at.elapsed() > std::time::Duration::from_secs(300))
        .map(|entry| entry.key().clone())
        .collect();

    for key in expired_keys {
        challenges.remove(&key);
    }
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

pub static CAPABILITY_DESCRIPTIONS: &[(&str, &str)] = &[
    ("play_media", "Play any accessible media"),
    ("can_transcode", "Request transcoded streams"),
    ("can_download", "Download media files"),
    ("can_delete_media", "Delete media from disk"),
    (
        "can_manage_libraries",
        "Create, edit, scan, and delete libraries",
    ),
    ("can_manage_users", "Create, edit, and delete users"),
    (
        "can_view_analytics",
        "Access analytics dashboard and play history",
    ),
    (
        "can_manage_server",
        "Access server settings, configuration, and logs",
    ),
    (
        "can_manage_scheduled_tasks",
        "Create, edit, and trigger scheduled tasks",
    ),
    ("can_use_live_tv", "Access live TV features"),
    ("can_share_content", "Share content links externally"),
    (
        "can_remote_control",
        "Remote control other users playback sessions",
    ),
];

pub fn validate_capability_name(name: &str) -> bool {
    ALL_CAPABILITIES.contains(&name)
}

pub fn check_capability(
    role: &str,
    capabilities: &[String],
    required: &str,
) -> Result<(), AuthError> {
    if role == "owner" {
        return Ok(());
    }
    if capabilities.iter().any(|c| c == required) {
        return Ok(());
    }
    Err(AuthError::InsufficientCapabilities {
        required: vec![required.to_string()],
    })
}

pub async fn get_capability_overrides(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<super::types::CapabilityOverrideResponse>, AuthError> {
    let rows = sqlx::query(
        "SELECT capability, is_granted FROM user_capabilities WHERE user_id = $1 ORDER BY capability",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| super::types::CapabilityOverrideResponse {
            capability: row.get("capability"),
            is_granted: row.get("is_granted"),
        })
        .collect())
}

pub async fn update_capabilities(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    overrides: Vec<super::types::CapabilityOverride>,
    role: &str,
) -> Result<Vec<super::types::CapabilityOverrideResponse>, AuthError> {
    for ov in &overrides {
        if !validate_capability_name(&ov.capability) {
            return Err(AuthError::InsufficientCapabilities {
                required: vec![format!("invalid capability: {}", ov.capability)],
            });
        }
    }

    if role == "owner" {
        return get_capability_overrides(pool, user_id).await;
    }

    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM user_capabilities WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;

    for ov in &overrides {
        sqlx::query(
            "INSERT INTO user_capabilities (user_id, capability, is_granted) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(&ov.capability)
        .bind(ov.is_granted)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    get_capability_overrides(pool, user_id).await
}

const BASE20_CHARS: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ";

pub fn generate_invite_code() -> String {
    let mut rng = rand::rng();
    let chars: Vec<u8> = (0..24)
        .map(|_| {
            let idx: usize = rng.random_range(0..BASE20_CHARS.len());
            BASE20_CHARS[idx]
        })
        .collect();

    let raw = String::from_utf8(chars).unwrap();
    let formatted: String = raw
        .as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("-");

    format!("mv_invite-{}", formatted)
}

pub fn generate_reauth_code() -> String {
    let mut rng = rand::rng();
    let chars: Vec<u8> = (0..16)
        .map(|_| {
            let idx: usize = rng.random_range(0..BASE20_CHARS.len());
            BASE20_CHARS[idx]
        })
        .collect();

    let raw = String::from_utf8(chars).unwrap();
    let formatted: String = raw
        .as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).unwrap())
        .collect::<Vec<_>>()
        .join("-");

    format!("mv_reauth-{}", formatted)
}

fn generate_device_user_code(length: usize) -> String {
    let mut rng = rand::rng();
    let chars: Vec<u8> = (0..length)
        .map(|_| {
            let idx: usize = rng.random_range(0..BASE20_CHARS.len());
            BASE20_CHARS[idx]
        })
        .collect();
    String::from_utf8(chars).unwrap()
}

fn format_user_code(code: &str) -> String {
    let mid = code.len() / 2;
    format!("{}-{}", &code[..mid], &code[mid..])
}

pub struct CreateDeviceCodeParams {
    pub device_id: Option<String>,
    pub client_name: Option<String>,
    pub client_platform: Option<String>,
    pub client_version: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub verification_uri: String,
}

pub async fn create_device_linking_code(
    pool: &sqlx::PgPool,
    state: &AppState,
    params: CreateDeviceCodeParams,
) -> Result<DeviceCodeResponse, AuthError> {
    let config = state.runtime_config.load();
    let code_length = config.auth.device_linking_code_length;
    let expiry_seconds = config.auth.device_linking_code_expiry_seconds;
    let poll_interval = config.auth.device_linking_poll_interval_seconds;
    drop(config);

    let raw_user_code = generate_device_user_code(code_length);
    let formatted_user_code = format_user_code(&raw_user_code);

    let device_code_raw = generate_session_token();
    let device_code_hash = sha256_hex(&device_code_raw);

    sqlx::query(
        r#"
        INSERT INTO device_linking_codes (
            user_code, device_code, device_id, client_name, client_platform, client_version,
            ip_address, user_agent, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7::inet, $8, now() + ($9 || ' seconds')::interval)
        "#,
    )
    .bind(&raw_user_code)
    .bind(&device_code_hash)
    .bind(&params.device_id)
    .bind(&params.client_name)
    .bind(&params.client_platform)
    .bind(&params.client_version)
    .bind(&params.ip_address)
    .bind(&params.user_agent)
    .bind(format!("{}", expiry_seconds))
    .execute(pool)
    .await?;

    Ok(DeviceCodeResponse {
        device_code: device_code_raw,
        user_code: formatted_user_code,
        verification_uri: params.verification_uri,
        expires_in: expiry_seconds,
        interval: poll_interval,
    })
}

pub async fn poll_device_linking_token(
    pool: &sqlx::PgPool,
    state: &AppState,
    device_code_raw: &str,
) -> Result<DeviceTokenResponse, AuthError> {
    let device_code_hash = sha256_hex(device_code_raw);

    let row = sqlx::query(
        r#"
        SELECT id, device_id, client_name, client_platform, client_version,
               ip_address::text as ip_address, user_agent, expires_at,
               is_approved, approved_by_user_id
        FROM device_linking_codes
        WHERE device_code = $1
        "#,
    )
    .bind(&device_code_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::DeviceLinkingExpired)?;

    let linking_id: Uuid = row.get("id");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    if expires_at < chrono::Utc::now() {
        let _ = sqlx::query("DELETE FROM device_linking_codes WHERE id = $1")
            .bind(linking_id)
            .execute(pool)
            .await;
        return Err(AuthError::DeviceLinkingExpired);
    }

    let is_approved: bool = row.get("is_approved");
    if !is_approved {
        return Err(AuthError::DeviceLinkingPending);
    }

    let approved_by_user_id: Uuid = row
        .try_get("approved_by_user_id")
        .map_err(|_| AuthError::DeviceLinkingDenied)?;

    let device_info = DeviceInfo {
        device_id: row.try_get("device_id").ok(),
        device_name: None,
        client_name: row.try_get("client_name").ok(),
        client_version: row.try_get("client_version").ok(),
        client_platform: row.try_get("client_platform").ok(),
        ip_address: row.try_get("ip_address").ok(),
        user_agent: row.try_get("user_agent").ok(),
        is_secure: false,
    };

    let (token, session) = create_session(pool, state, approved_by_user_id, &device_info).await?;

    let user = sqlx::query(
        r#"SELECT id, username, display_name, role, has_all_library_access FROM users WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(approved_by_user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::InvalidCredentials)?;

    let db_user_id: Uuid = user.get("id");
    let username: String = user.get("username");
    let display_name: String = user.get("display_name");
    let role: String = user.get("role");
    let has_all_library_access: bool = user.get("has_all_library_access");

    let capabilities = resolve_capabilities(pool, db_user_id, &role).await?;

    let _ = sqlx::query("DELETE FROM device_linking_codes WHERE id = $1")
        .bind(linking_id)
        .execute(pool)
        .await;

    Ok(DeviceTokenResponse {
        session_token: token,
        user: UserSummary {
            id: db_user_id,
            username,
            display_name,
            role,
            capabilities,
            has_all_library_access,
            active_profile_id: session.active_profile_id,
        },
    })
}

pub async fn verify_device_linking_code(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    user_code: &str,
) -> Result<serde_json::Value, AuthError> {
    let normalized: String = user_code.chars().filter(|c| *c != '-').collect();

    let row = sqlx::query(
        r#"
        SELECT id, client_name, client_platform, client_version, expires_at, is_approved
        FROM device_linking_codes
        WHERE user_code = $1
        "#,
    )
    .bind(&normalized)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::DeviceLinkingExpired)?;

    let linking_id: Uuid = row.get("id");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    if expires_at < chrono::Utc::now() {
        let _ = sqlx::query("DELETE FROM device_linking_codes WHERE id = $1")
            .bind(linking_id)
            .execute(pool)
            .await;
        return Err(AuthError::DeviceLinkingExpired);
    }

    let is_approved: bool = row.get("is_approved");
    if is_approved {
        return Err(AuthError::DeviceLinkingExpired);
    }

    let client_name: Option<String> = row.try_get("client_name").ok();
    let client_platform: Option<String> = row.try_get("client_platform").ok();
    let client_version: Option<String> = row.try_get("client_version").ok();

    sqlx::query(
        r#"
        UPDATE device_linking_codes
        SET is_approved = true, approved_by_user_id = $2, approved_at = now()
        WHERE id = $1
        "#,
    )
    .bind(linking_id)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(serde_json::json!({
        "status": "approved",
        "device": {
            "client_name": client_name,
            "client_platform": client_platform,
            "client_version": client_version,
        }
    }))
}

fn extract_code_prefix(full_code: &str) -> String {
    let stripped = full_code.strip_prefix("mv_invite-").unwrap_or(full_code);
    let no_dashes: String = stripped.chars().filter(|c| *c != '-').collect();
    no_dashes[..4].to_string()
}

pub struct CreateInvitationParams {
    pub admin_user_id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub role: String,
    pub capabilities: Vec<String>,
    pub library_ids: Vec<Uuid>,
    pub has_all_library_access: bool,
    pub max_uses: i32,
    pub expires_at: Option<DateTime<Utc>>,
}

pub async fn create_invitation(
    pool: &sqlx::PgPool,
    params: CreateInvitationParams,
) -> Result<(String, InvitationResponse), AuthError> {
    let raw_code = generate_invite_code();
    let code_hash = sha256_hex(&raw_code);
    let code_prefix = extract_code_prefix(&raw_code);

    let capabilities_json = serde_json::to_value(&params.capabilities).unwrap_or_default();
    let library_ids_json = serde_json::to_value(&params.library_ids).unwrap_or_default();

    let row = sqlx::query(
        r#"
        INSERT INTO invitations (
            created_by_user_id, code_hash, code_prefix, email, display_name,
            role, capabilities, library_ids, has_all_library_access,
            max_uses, expires_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, code_prefix, email, display_name, role, max_uses,
                  use_count, expires_at, is_revoked, created_at
        "#,
    )
    .bind(params.admin_user_id)
    .bind(&code_hash)
    .bind(&code_prefix)
    .bind(&params.email)
    .bind(&params.display_name)
    .bind(&params.role)
    .bind(&capabilities_json)
    .bind(&library_ids_json)
    .bind(params.has_all_library_access)
    .bind(params.max_uses)
    .bind(params.expires_at)
    .fetch_one(pool)
    .await?;

    let invitation_id: Uuid = row.get("id");
    let resp = InvitationResponse {
        id: invitation_id,
        code: Some(raw_code.clone()),
        code_prefix: row.get("code_prefix"),
        email: row.get("email"),
        display_name: row.try_get("display_name").ok(),
        role: row.get("role"),
        max_uses: row.get("max_uses"),
        use_count: row.get("use_count"),
        expires_at: row.try_get("expires_at").ok(),
        is_revoked: row.get("is_revoked"),
        created_at: row.get("created_at"),
    };

    Ok((raw_code, resp))
}

pub async fn list_invitations(
    pool: &sqlx::PgPool,
) -> Result<(Vec<InvitationResponse>, i64), AuthError> {
    let count_row = sqlx::query("SELECT count(*) as cnt FROM invitations")
        .fetch_one(pool)
        .await?;
    let total: i64 = count_row.get("cnt");

    let rows = sqlx::query(
        r#"
        SELECT id, code_prefix, email, display_name, role, max_uses,
               use_count, expires_at, is_revoked, created_at
        FROM invitations
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|row| InvitationResponse {
            id: row.get("id"),
            code: None,
            code_prefix: row.get("code_prefix"),
            email: row.get("email"),
            display_name: row.try_get("display_name").ok(),
            role: row.get("role"),
            max_uses: row.get("max_uses"),
            use_count: row.get("use_count"),
            expires_at: row.try_get("expires_at").ok(),
            is_revoked: row.get("is_revoked"),
            created_at: row.get("created_at"),
        })
        .collect();

    Ok((items, total))
}

pub async fn revoke_invitation(
    pool: &sqlx::PgPool,
    invitation_id: Uuid,
) -> Result<InvitationResponse, AuthError> {
    let row = sqlx::query(
        r#"
        UPDATE invitations SET is_revoked = true
        WHERE id = $1 AND is_revoked = false
        RETURNING id, code_prefix, email, display_name, role, max_uses,
                  use_count, expires_at, is_revoked, created_at
        "#,
    )
    .bind(invitation_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::InviteCodeRevoked)?;

    Ok(InvitationResponse {
        id: row.get("id"),
        code: None,
        code_prefix: row.get("code_prefix"),
        email: row.get("email"),
        display_name: row.try_get("display_name").ok(),
        role: row.get("role"),
        max_uses: row.get("max_uses"),
        use_count: row.get("use_count"),
        expires_at: row.try_get("expires_at").ok(),
        is_revoked: row.get("is_revoked"),
        created_at: row.get("created_at"),
    })
}

pub async fn resend_invitation(
    pool: &sqlx::PgPool,
    invitation_id: Uuid,
) -> Result<InvitationResponse, AuthError> {
    let existing = sqlx::query(
        r#"
        SELECT id, code_hash, code_prefix, email, display_name, role, max_uses,
               use_count, expires_at, is_revoked, created_at
        FROM invitations
        WHERE id = $1
        "#,
    )
    .bind(invitation_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::InviteCodeInvalid)?;

    let is_revoked: bool = existing.get("is_revoked");
    if is_revoked {
        return Err(AuthError::InviteCodeRevoked);
    }

    let new_code = generate_invite_code();
    let new_hash = sha256_hex(&new_code);
    let new_prefix = extract_code_prefix(&new_code);

    let row = sqlx::query(
        r#"
        UPDATE invitations SET code_hash = $2, code_prefix = $3, use_count = 0
        WHERE id = $1
        RETURNING id, code_prefix, email, display_name, role, max_uses,
                  use_count, expires_at, is_revoked, created_at
        "#,
    )
    .bind(invitation_id)
    .bind(&new_hash)
    .bind(&new_prefix)
    .fetch_one(pool)
    .await?;

    tracing::info!(
        invitation_id = %invitation_id,
        email = %row.get::<String, _>("email"),
        "invite code regenerated for resend (SMTP delivery not yet implemented)"
    );

    Ok(InvitationResponse {
        id: row.get("id"),
        code: Some(new_code),
        code_prefix: row.get("code_prefix"),
        email: row.get("email"),
        display_name: row.try_get("display_name").ok(),
        role: row.get("role"),
        max_uses: row.get("max_uses"),
        use_count: row.get("use_count"),
        expires_at: row.try_get("expires_at").ok(),
        is_revoked: row.get("is_revoked"),
        created_at: row.get("created_at"),
    })
}

fn extract_reauth_prefix(full_code: &str) -> String {
    let stripped = full_code.strip_prefix("mv_reauth-").unwrap_or(full_code);
    let no_dashes: String = stripped.chars().filter(|c| *c != '-').collect();
    no_dashes[..4].to_string()
}

pub async fn create_reauth_code(
    pool: &sqlx::PgPool,
    state: &AppState,
    user_id: Uuid,
    requested_by_user_id: Uuid,
    ip_address: Option<String>,
) -> Result<ReauthCodeResponse, AuthError> {
    let config = state.runtime_config.load();
    let expiry_hours = config.auth.reauth_code_expiry_hours;
    let max_requests = config.auth.reauth_max_requests_per_user_per_day;
    drop(config);

    let count_row = sqlx::query(
        r#"
        SELECT count(*) as cnt FROM reauth_codes
        WHERE user_id = $1 AND created_at > now() - '24 hours'::interval
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    let recent_count: i64 = count_row.get("cnt");
    if recent_count >= max_requests as i64 {
        return Err(AuthError::ReauthRateLimited);
    }

    let raw_code = generate_reauth_code();
    let code_hash = sha256_hex(&raw_code);
    let code_prefix = extract_reauth_prefix(&raw_code);

    let row = sqlx::query(
        r#"
        INSERT INTO reauth_codes (
            user_id, requested_by_user_id, code_hash, code_prefix,
            ip_address, expires_at
        ) VALUES ($1, $2, $3, $4, $5::inet, now() + ($6 || ' hours')::interval)
        RETURNING code_prefix, expires_at
        "#,
    )
    .bind(user_id)
    .bind(requested_by_user_id)
    .bind(&code_hash)
    .bind(&code_prefix)
    .bind(&ip_address)
    .bind(format!("{}", expiry_hours))
    .fetch_one(pool)
    .await?;

    let resp_prefix: String = row.get("code_prefix");
    let expires_at: DateTime<Utc> = row.get("expires_at");

    tracing::info!(
        user_id = %user_id,
        prefix = %resp_prefix,
        "re-auth code generated (SMTP delivery not yet implemented)"
    );

    Ok(ReauthCodeResponse {
        prefix: resp_prefix,
        expires_at,
    })
}

pub async fn authenticate_reauth_code(
    pool: &sqlx::PgPool,
    state: &AppState,
    code: &str,
    device_info: &DeviceInfo,
) -> Result<(String, UserSummary), AuthError> {
    let code_hash = sha256_hex(code);

    let row = sqlx::query(
        r#"
        SELECT id, user_id, code_hash, code_prefix, expires_at, is_used
        FROM reauth_codes
        WHERE code_hash = $1
        "#,
    )
    .bind(&code_hash)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::ReauthCodeInvalid)?;

    let reauth_id: Uuid = row.get("id");
    let user_id: Uuid = row.get("user_id");
    let expires_at: DateTime<Utc> = row.get("expires_at");
    let is_used: bool = row.get("is_used");

    if is_used {
        return Err(AuthError::ReauthCodeInvalid);
    }

    if expires_at < chrono::Utc::now() {
        return Err(AuthError::ReauthCodeInvalid);
    }

    let user = sqlx::query(
        r#"SELECT id, username, display_name, role, has_all_library_access, status
           FROM users WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AuthError::ReauthCodeInvalid)?;

    let status: String = user.get("status");
    if status != "active" {
        return Err(AuthError::ReauthCodeInvalid);
    }

    let (token, session) = create_session(pool, state, user_id, device_info).await?;

    sqlx::query(
        r#"UPDATE reauth_codes SET is_used = true, used_at = now(), resulting_session_id = $2 WHERE id = $1"#,
    )
    .bind(reauth_id)
    .bind(session.id)
    .execute(pool)
    .await?;

    let db_user_id: Uuid = user.get("id");
    let username: String = user.get("username");
    let display_name: String = user.get("display_name");
    let role: String = user.get("role");
    let has_all_library_access: bool = user.get("has_all_library_access");

    let capabilities = resolve_capabilities(pool, db_user_id, &role).await?;

    Ok((
        token,
        UserSummary {
            id: db_user_id,
            username,
            display_name,
            role,
            capabilities,
            has_all_library_access,
            active_profile_id: session.active_profile_id,
        },
    ))
}
