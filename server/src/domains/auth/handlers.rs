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
use axum::http::HeaderMap;
use axum::Json;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

use super::error::AuthError;
use super::service;
use super::service::DeviceInfo;
use super::types::*;

pub async fn setup(
    State(state): State<AppState>,
    Json(req): Json<SetupRequest>,
) -> Result<Json<SessionResponse>, AppError> {
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

    Ok(Json(SessionResponse {
        session_token: token,
        user: UserSummary {
            id: user_id,
            username: "".to_string(),
            display_name: "".to_string(),
            role: "owner".to_string(),
            capabilities: service::ALL_CAPABILITIES.iter().map(|s| s.to_string()).collect(),
            has_all_library_access: true,
        },
    }))
}

pub async fn auth_invite(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<InviteAuthRequest>,
) -> Result<Json<SessionResponse>, AppError> {
    let device_info = extract_device_info(
        &headers,
        req.device_name.as_deref(),
        req.client_name.as_deref(),
        req.client_version.as_deref(),
        req.client_platform.as_deref(),
    );

    let (_user_id, token, summary) =
        service::authenticate_invite_code(&state.pool, &state, &req.code, &device_info).await?;

    Ok(Json(SessionResponse {
        session_token: token,
        user: summary,
    }))
}

pub async fn auth_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PasswordLoginRequest>,
) -> Result<Json<SessionResponse>, AppError> {
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

    Ok(Json(SessionResponse {
        session_token: token,
        user: UserSummary {
            id: user.id,
            username: user.username,
            display_name: user.display_name,
            role: user.role,
            capabilities,
            has_all_library_access: user.has_all_library_access,
        },
    }))
}

pub async fn auth_logout(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, AppError> {
    service::revoke_session(&state.pool, user.session_id, user.user_id).await?;
    Ok(Json(serde_json::json!({ "status": "logged_out" })))
}

pub async fn auth_logout_all(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, AppError> {
    let count = service::revoke_all_sessions(&state.pool, user.user_id).await?;
    Ok(Json(serde_json::json!({
        "status": "logged_out_everywhere",
        "sessions_revoked": count,
    })))
}

pub async fn webauthn_start(
    State(_state): State<AppState>,
    Json(_req): Json<WebauthnStartRequest>,
) -> Result<Json<WebauthnChallengeResponse>, AppError> {
    todo!("Task 2: WebAuthn registration/authentication flow");
}

pub async fn webauthn_finish(
    State(_state): State<AppState>,
    Json(_req): Json<WebauthnFinishRequest>,
) -> Result<Json<SessionResponse>, AppError> {
    todo!("Task 2: WebAuthn registration/authentication flow");
}

pub async fn totp_verify(
    State(_state): State<AppState>,
    Json(_req): Json<TotpVerifyRequest>,
) -> Result<Json<SessionResponse>, AppError> {
    todo!("Task 4: TOTP verification");
}

pub async fn device_code(
    State(_state): State<AppState>,
    _headers: HeaderMap,
    Json(_req): Json<DeviceCodeRequest>,
) -> Result<Json<DeviceCodeResponse>, AppError> {
    todo!("Task 6: Device linking code creation");
}

pub async fn device_token(
    State(_state): State<AppState>,
    Json(_req): Json<DeviceTokenRequest>,
) -> Result<Json<DeviceTokenResponse>, AppError> {
    todo!("Task 6: Device linking token polling");
}

pub async fn device_verify(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<DeviceVerifyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!("Task 6: Device linking verification");
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
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<PasskeyListResponse>, AppError> {
    todo!("Task 2: Passkey listing");
}

pub async fn passkey_register_start(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<PasskeyRegisterStartRequest>,
) -> Result<Json<WebauthnChallengeResponse>, AppError> {
    todo!("Task 2: Passkey registration start");
}

pub async fn passkey_register_finish(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<PasskeyRegisterFinishRequest>,
) -> Result<Json<PasskeyResponse>, AppError> {
    todo!("Task 2: Passkey registration finish");
}

pub async fn passkey_delete(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    axum::extract::Path(_passkey_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!("Task 2: Passkey deletion");
}

pub async fn list_invitations(
    State(_state): State<AppState>,
) -> Result<Json<InvitationListResponse>, AppError> {
    todo!("Task 3: Invitation listing");
}

pub async fn create_invitation(
    State(_state): State<AppState>,
    Json(_req): Json<CreateInvitationRequest>,
) -> Result<Json<InvitationResponse>, AppError> {
    todo!("Task 3: Invitation creation");
}

pub async fn revoke_invitation(
    State(_state): State<AppState>,
    axum::extract::Path(_invitation_id): axum::extract::Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!("Task 3: Invitation revocation");
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
