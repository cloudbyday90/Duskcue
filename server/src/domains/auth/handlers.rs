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

use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use sqlx::Row;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::state::{AppState, NetworkMode};

use super::types::*;
use super::error::AuthError;
use super::service;
use super::service::DeviceInfo;

fn build_session_cookie_value(
    state: &AppState,
    token: &str,
    max_age_days: i32,
) -> String {
    let config = state.runtime_config.load();
    let is_exposed = matches!(config.auth.network_mode, NetworkMode::Exposed);
    drop(config);

    let max_age_seconds = max_age_days as i64 * 86400;

    let mut cookie = format!(
        "session={}; Path=/; HttpOnly; SameSite=Strict; Max-Age={}",
        token, max_age_seconds,
    );

    if is_exposed {
        cookie.push_str("; Secure");
    }

    cookie
}

fn set_session_cookie(
    state: &AppState,
    headers: &mut HeaderMap,
    token: &str,
) {
    let config = state.runtime_config.load();
    let max_age_days = config.auth.session_absolute_timeout_days;
    drop(config);

    let cookie_value = build_session_cookie_value(state, token, max_age_days);
    if let Ok(value) = axum::http::HeaderValue::from_str(&cookie_value) {
        headers.insert(SET_COOKIE, value);
    }
}

fn clear_session_cookie(state: &AppState, headers: &mut HeaderMap) {
    let config = state.runtime_config.load();
    let is_exposed = matches!(config.auth.network_mode, NetworkMode::Exposed);
    drop(config);

    let mut cookie = "session=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0".to_string();
    if is_exposed {
        cookie.push_str("; Secure");
    }
    if let Ok(value) = axum::http::HeaderValue::from_str(&cookie) {
        headers.insert(SET_COOKIE, value);
    }
}

struct CookieResponse {
    body: serde_json::Value,
    status: axum::http::StatusCode,
}

impl IntoResponse for CookieResponse {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

struct SessionResponseWrapper {
    inner: SessionResponse,
}

impl IntoResponse for SessionResponseWrapper {
    fn into_response(self) -> Response {
        Json(self.inner).into_response()
    }
}

pub async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.pool;

    if service::is_setup_complete(pool).await? {
        return Err(AuthError::SetupAlreadyComplete.into());
    }

    if service::user_count(pool).await? > 0 {
        return Err(AuthError::SetupAlreadyComplete.into());
    }

    req.validate().map_err(|e| {
        AppError::Validation {
            errors: e
                .field_errors()
                .into_iter()
                .flat_map(|(field, errors)| {
                    errors.iter().map(move |err| crate::error::FieldError {
                        field: field.to_string(),
                        code: err.code.to_string(),
                        message: err
                            .message
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_default(),
                    })
                })
                .collect(),
            instance: Some("/api/v1/setup".to_string()),
        }
    })?;

    let (user_id, token) =
        service::setup_owner(pool, req.username, req.display_name, req.password).await?;

    let body = SessionResponse {
        session_token: token.clone(),
        user: UserSummary {
            id: user_id,
            username: "".to_string(),
            display_name: "".to_string(),
            role: "owner".to_string(),
            capabilities: service::ALL_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            has_all_library_access: true,
        },
    };

    let mut response = SessionResponseWrapper { inner: body }.into_response();
    set_session_cookie(&state, response.headers_mut(), &token);
    Ok(response)
}

pub async fn auth_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InviteAuthRequest>,
) -> Result<impl IntoResponse, AppError> {
    let device_info = extract_device_info(
        &headers,
        req.device_name.as_deref(),
        req.client_name.as_deref(),
        req.client_version.as_deref(),
        req.client_platform.as_deref(),
    );

    let (_user_id, token, summary) =
        service::authenticate_invite_code(&state.pool, &state, &req.code, &device_info).await?;

    let body = SessionResponse {
        session_token: token.clone(),
        user: summary,
    };

    let mut response = SessionResponseWrapper { inner: body }.into_response();
    set_session_cookie(&state, response.headers_mut(), &token);
    Ok(response)
}

pub async fn auth_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PasswordLoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let device_info = extract_device_info(
        &headers,
        req.device_name.as_deref(),
        req.client_name.as_deref(),
        req.client_version.as_deref(),
        req.client_platform.as_deref(),
    );

    let user = service::get_user_for_login(&state.pool, &req.username)
        .await?
        .ok_or(AuthError::InvalidCredentials)?;

    if user.status == "locked"
        && let Some(until) = user.locked_until
        && until > chrono::Utc::now()
    {
        return Err(AppError::Auth(AuthError::AccountLocked { until }));
    }

    let password_hash = user.password_hash.ok_or(AuthError::InvalidCredentials)?;

    service::verify_password_hash(&password_hash, &req.password)?;

    let (token, _session) =
        service::create_session(&state.pool, &state, user.id, &device_info).await?;

    service::reset_login_failures(&state.pool, user.id).await?;

    let capabilities = service::resolve_capabilities(&state.pool, user.id, &user.role).await?;

    let body = SessionResponse {
        session_token: token.clone(),
        user: UserSummary {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            capabilities,
            has_all_library_access: user.has_all_library_access,
        },
    };

    let mut response = SessionResponseWrapper { inner: body }.into_response();
    set_session_cookie(&state, response.headers_mut(), &token);
    Ok(response)
}

pub async fn auth_logout(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<impl IntoResponse, AppError> {
    service::revoke_session(&state.pool, user.session_id, user.user_id).await?;
    let mut response = CookieResponse {
        body: serde_json::json!({ "status": "logged_out" }),
        status: axum::http::StatusCode::OK,
    }.into_response();
    clear_session_cookie(&state, response.headers_mut());
    Ok(response)
}

pub async fn auth_logout_all(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<impl IntoResponse, AppError> {
    let count = service::revoke_all_sessions(&state.pool, user.user_id).await?;
    let mut response = CookieResponse {
        body: serde_json::json!({
            "status": "logged_out_everywhere",
            "sessions_revoked": count,
        }),
        status: axum::http::StatusCode::OK,
    }.into_response();
    clear_session_cookie(&state, response.headers_mut());
    Ok(response)
}

pub async fn webauthn_start(
    State(state): State<AppState>,
    Json(req): Json<WebauthnStartRequest>,
) -> Result<Json<WebauthnAuthStartResponse>, AppError> {
    service::expire_challenges(&state.webauthn_challenges);

    let (challenge_id, request_options) =
        service::start_passkey_authentication(&state).await?;

    let _username_hint = req.username.as_deref();

    Ok(Json(WebauthnAuthStartResponse {
        challenge_id,
        request_options,
    }))
}

pub async fn webauthn_finish(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<WebauthnFinishRequest>,
) -> Result<impl IntoResponse, AppError> {
    let challenge_id = headers
        .get("x-challenge-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if challenge_id.is_empty() {
        return Err(AppError::Auth(AuthError::WebauthnChallengeExpired));
    }

    let device_info = DeviceInfo {
        device_id: None,
        device_name: None,
        client_name: None,
        client_version: None,
        client_platform: None,
        ip_address: headers
            .get("x-forwarded-for")
            .or_else(|| headers.get("x-real-ip"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string()),
        user_agent: headers
            .get("user-agent")
            .and_then(|v| v.to_str().ok())
            .map(String::from),
        is_secure: false,
    };

    let (token, summary) = service::finish_passkey_authentication(
        &state,
        &challenge_id,
        &req.credential,
        &device_info,
    )
    .await?;

    let body = SessionResponse {
        session_token: token.clone(),
        user: summary,
    };

    let mut response = SessionResponseWrapper { inner: body }.into_response();
    set_session_cookie(&state, response.headers_mut(), &token);
    Ok(response)
}

pub async fn totp_verify(
    State(_state): State<AppState>,
    Json(_req): Json<TotpVerifyRequest>,
) -> Result<Json<SessionResponse>, AppError> {
    todo!("Task 4: TOTP verification");
}

pub async fn device_code(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponse>, AppError> {
    let ip_address = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string());

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let verification_uri = {
        let config = state.runtime_config.load();
        let base = match &config.base_url {
            Some(url) => url.clone(),
            None => headers
                .get("host")
                .and_then(|v| v.to_str().ok())
                .map(|h| format!("http://{}", h))
                .unwrap_or_else(|| "http://localhost:48027".to_string()),
        };
        drop(config);
        format!("{}/link", base.trim_end_matches('/'))
    };

    let response = service::create_device_linking_code(
        &state.pool,
        &state,
        service::CreateDeviceCodeParams {
            client_name: req.client_name,
            client_platform: req.client_platform,
            client_version: req.client_version,
            ip_address,
            user_agent,
            verification_uri,
        },
    )
    .await?;

    Ok(Json(response))
}

pub async fn device_token(
    State(state): State<AppState>,
    Json(req): Json<DeviceTokenRequest>,
) -> Result<Json<DeviceTokenResponse>, AppError> {
    let response = service::poll_device_linking_token(
        &state.pool,
        &state,
        &req.device_code,
    )
    .await?;

    Ok(Json(response))
}

pub async fn device_verify(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<DeviceVerifyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let result = service::verify_device_linking_code(
        &state.pool,
        user.user_id,
        &req.user_code,
    )
    .await?;

    Ok(Json(result))
}

pub async fn reauth(
    State(_state): State<AppState>,
    _headers: HeaderMap,
    Json(_req): Json<ReauthRequest>,
) -> Result<Json<SessionResponse>, AppError> {
    todo!("Task 7: Re-authentication code flow");
}

pub async fn reauth_request(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<ReauthCodeResponse>, AppError> {
    todo!("Task 7: Re-auth code generation");
}

pub async fn list_user_sessions(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<SessionListResponse>, AppError> {
    let sessions = service::list_user_sessions(&state.pool, user.user_id).await?;

    Ok(Json(SessionListResponse {
        items: sessions
            .into_iter()
            .map(|s| SessionDetailResponse {
                id: s.id,
                device_name: s.device_name,
                client_name: s.client_name,
                client_version: s.client_version,
                client_platform: s.client_platform,
                ip_address: s.ip_address,
                is_secure: s.is_secure,
                last_active_at: s.last_active_at,
                created_at: {
                    let millis: i64 = s.id.get_timestamp().map(|ts| {
                        let (secs, nanos) = ts.to_unix();
                        secs as i64 * 1000 + (nanos / 1_000_000) as i64
                    }).unwrap_or(0);
                    chrono::DateTime::from_timestamp_millis(millis).unwrap_or(s.last_active_at)
                },
            })
            .collect(),
    }))
}

pub async fn delete_user_session(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(session_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::revoke_session(&state.pool, session_id, user.user_id).await?;
    Ok(Json(serde_json::json!({ "status": "revoked" })))
}

pub async fn passkey_list(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<PasskeyListResponse>, AppError> {
    let passkeys = service::list_user_passkeys(&state.pool, user.user_id).await?;
    Ok(Json(PasskeyListResponse { items: passkeys }))
}

pub async fn passkey_register_start(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<PasskeyRegisterStartRequest>,
) -> Result<Json<WebauthnRegisterStartResponse>, AppError> {
    req.validate().map_err(|e| {
        AppError::Validation {
            errors: e
                .field_errors()
                .into_iter()
                .flat_map(|(field, errors)| {
                    errors.iter().map(move |err| crate::error::FieldError {
                        field: field.to_string(),
                        code: err.code.to_string(),
                        message: err
                            .message
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_default(),
                    })
                })
                .collect(),
            instance: Some("/api/v1/user/passkeys/register/start".to_string()),
        }
    })?;

    service::expire_challenges(&state.webauthn_challenges);

    let user_row = sqlx::query(
        "SELECT username, display_name FROM users WHERE id = $1",
    )
    .bind(user.user_id)
    .fetch_one(&state.pool)
    .await
    .map_err(|_| AuthError::InvalidCredentials)?;

    let username: String = user_row.try_get("username").unwrap_or_default();
    let display_name: String = user_row.try_get("display_name").unwrap_or_default();

    let (challenge_id, creation_options) = service::start_passkey_registration(
        &state,
        user.user_id,
        &username,
        &display_name,
        &req.name,
    )
    .await?;

    Ok(Json(WebauthnRegisterStartResponse {
        creation_options,
        challenge_id,
    }))
}

pub async fn passkey_register_finish(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    headers: HeaderMap,
    Json(req): Json<PasskeyRegisterFinishRequest>,
) -> Result<Json<PasskeyResponse>, AppError> {
    let challenge_id = headers
        .get("x-challenge-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if challenge_id.is_empty() {
        return Err(AppError::Auth(AuthError::WebauthnChallengeExpired));
    }

    if let Some(entry) = state.webauthn_challenges.get(&challenge_id) {
        if entry.user_id != Some(user.user_id) {
            return Err(AppError::Auth(AuthError::WebauthnChallengeExpired));
        }
        drop(entry);
    }

    let passkey = service::finish_passkey_registration(
        &state,
        &challenge_id,
        &req.credential,
        "New Passkey",
    )
    .await?;

    Ok(Json(passkey))
}

pub async fn passkey_delete(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(passkey_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_passkey(&state.pool, passkey_id, user.user_id).await?;
    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

pub async fn list_invitations(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<InvitationListResponse>, AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    let (items, total) = service::list_invitations(&state.pool).await?;

    Ok(Json(InvitationListResponse { items, total }))
}

pub async fn create_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateInvitationRequest>,
) -> Result<(axum::http::StatusCode, Json<InvitationResponse>), AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    req.validate().map_err(|e| {
        AppError::Validation {
            errors: e
                .field_errors()
                .into_iter()
                .flat_map(|(field, errors)| {
                    errors.iter().map(move |err| crate::error::FieldError {
                        field: field.to_string(),
                        code: err.code.to_string(),
                        message: err
                            .message
                            .as_ref()
                            .map(|m| m.to_string())
                            .unwrap_or_default(),
                    })
                })
                .collect(),
            instance: Some("/api/v1/invitations".to_string()),
        }
    })?;

    let role = req
        .role
        .unwrap_or_else(|| "member".to_string());

    let capabilities = req.capabilities.unwrap_or_default();
    let library_ids = req.library_ids.unwrap_or_default();
    let has_all_library_access = req.has_all_library_access.unwrap_or(false);
    let max_uses = req.max_uses.unwrap_or(1);

    let (_raw_code, invitation) = service::create_invitation(
        &state.pool,
        service::CreateInvitationParams {
            admin_user_id: user.user_id,
            email: req.email,
            display_name: req.display_name,
            role,
            capabilities,
            library_ids,
            has_all_library_access,
            max_uses,
            expires_at: req.expires_at,
        },
    )
    .await?;

    Ok((axum::http::StatusCode::CREATED, Json(invitation)))
}

pub async fn revoke_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(invitation_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<InvitationResponse>, AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    let invitation = service::revoke_invitation(&state.pool, invitation_id).await?;
    Ok(Json(invitation))
}

pub async fn resend_invitation(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(invitation_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<InvitationResponse>, AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    let invitation = service::resend_invitation(&state.pool, invitation_id).await?;
    Ok(Json(invitation))
}

pub async fn list_capabilities() -> Result<Json<CapabilityListResponse>, AppError> {
    let capabilities = service::CAPABILITY_DESCRIPTIONS
        .iter()
        .map(|(name, description)| AvailableCapability {
            name: name.to_string(),
            description: description.to_string(),
        })
        .collect();

    Ok(Json(CapabilityListResponse { capabilities }))
}

pub async fn get_user_capabilities(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(target_user_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<CapabilityOverridesResponse>, AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    let target_user = sqlx::query(
        "SELECT id, role FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(target_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Auth(AuthError::Database(e)))?
    .ok_or(AppError::NotFound("User not found".into()))?;

    let role: String = target_user.get("role");
    let overrides = service::get_capability_overrides(&state.pool, target_user_id).await?;
    let effective = service::resolve_capabilities(&state.pool, target_user_id, &role).await?;

    Ok(Json(CapabilityOverridesResponse {
        user_id: target_user_id,
        role,
        overrides,
        effective,
    }))
}

pub async fn update_user_capabilities(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    axum::extract::Path(target_user_id): axum::extract::Path<uuid::Uuid>,
    Json(req): Json<UpdateCapabilitiesRequest>,
) -> Result<Json<CapabilityOverridesResponse>, AppError> {
    service::check_capability(&user.role, &user.capabilities, "can_manage_users")?;

    let target_user = sqlx::query(
        "SELECT id, role FROM users WHERE id = $1 AND deleted_at IS NULL",
    )
    .bind(target_user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Auth(AuthError::Database(e)))?
    .ok_or(AppError::NotFound("User not found".into()))?;

    let role: String = target_user.get("role");

    service::update_capabilities(&state.pool, target_user_id, req.capabilities, &role).await?;

    let overrides = service::get_capability_overrides(&state.pool, target_user_id).await?;
    let effective = service::resolve_capabilities(&state.pool, target_user_id, &role).await?;

    Ok(Json(CapabilityOverridesResponse {
        user_id: target_user_id,
        role,
        overrides,
        effective,
    }))
}

fn extract_device_info(
    headers: &HeaderMap,
    device_name: Option<&str>,
    client_name: Option<&str>,
    client_version: Option<&str>,
    client_platform: Option<&str>,
) -> DeviceInfo {
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let ip_address = headers
        .get("x-forwarded-for")
        .or_else(|| headers.get("x-real-ip"))
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string());

    DeviceInfo {
        device_id: None,
        device_name: device_name.map(String::from),
        client_name: client_name.map(String::from),
        client_version: client_version.map(String::from),
        client_platform: client_platform.map(String::from),
        ip_address,
        user_agent,
        is_secure: false,
    }
}
