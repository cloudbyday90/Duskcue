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

use std::borrow::Cow;
use std::sync::OnceLock;

use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

static ENVIRONMENT: OnceLock<String> = OnceLock::new();

pub fn set_environment(env: String) {
    let _ = ENVIRONMENT.set(env);
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct FieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ProblemDetail {
    #[serde(rename = "type")]
    pub r#type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<FieldError>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_detail: Option<String>,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error")]
    Validation { errors: Vec<FieldError>, instance: Option<String> },

    #[error("rate limit exceeded: {code}")]
    RateLimited { code: String },

    #[error("unauthorized")]
    Unauthorized(String),

    #[error("forbidden")]
    Forbidden(String),

    #[error("unprocessable entity: {0}")]
    UnprocessableEntity(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("gateway timeout: {0}")]
    GatewayTimeout(String),

    #[error(transparent)]
    Auth(#[from] crate::domains::auth::AuthError),

    #[error("internal server error")]
    Internal(#[source] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, detail): (StatusCode, &str, Cow<'_, str>) = match &self {
            AppError::Auth(e) => {
                let (s, c, d) = auth_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", Cow::Borrowed(msg.as_str())),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", Cow::Borrowed(msg.as_str())),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", Cow::Borrowed(msg.as_str())),
            AppError::Validation { .. } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALID_001",
                Cow::Borrowed("One or more fields failed validation"),
            ),
            AppError::RateLimited { code } => (StatusCode::TOO_MANY_REQUESTS, code.as_str(), Cow::Borrowed("Rate limit exceeded")),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", Cow::Borrowed(msg.as_str())),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", Cow::Borrowed(msg.as_str())),
            AppError::UnprocessableEntity(msg) => (StatusCode::UNPROCESSABLE_ENTITY, "UNPROCESSABLE_ENTITY", Cow::Borrowed(msg.as_str())),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE", Cow::Borrowed(msg.as_str())),
            AppError::GatewayTimeout(msg) => (StatusCode::GATEWAY_TIMEOUT, "GATEWAY_TIMEOUT", Cow::Borrowed(msg.as_str())),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", Cow::Borrowed("Internal server error")),
        };

        let detail = if status.as_u16() >= 500 && !is_development_env() {
            Cow::Borrowed("Internal server error")
        } else {
            detail
        };

        let trace_id = get_trace_id();

        let mut body = ProblemDetail {
            r#type: format!("/errors/{}", code.to_lowercase()),
            title: code.to_string(),
            status: status.as_u16(),
            detail: detail.into_owned(),
            trace_id,
            ..Default::default()
        };

        if let AppError::Validation { errors, instance } = &self {
            body.errors = Some(errors.clone());
            body.instance = instance.clone();
        }

        if is_development_env()
            && let AppError::Internal(ref err) = self
        {
            body.debug_detail = Some(format!("{:?}", err));
        }

        tracing::error!(
            trace_id = %body.trace_id,
            error_code = code,
            status = status.as_u16(),
            error = %self,
            "request error"
        );

        let mut response = (status, Json(body)).into_response();

        let wants_retry_after = matches!(
            &self,
            AppError::RateLimited { .. }
                | AppError::ServiceUnavailable(_)
                | AppError::GatewayTimeout(_)
        );

        if wants_retry_after {
            let retry_seconds = match &self {
                AppError::RateLimited { .. } => "0",
                AppError::GatewayTimeout(_) => "30",
                AppError::ServiceUnavailable(_) => "60",
                _ => "60",
            };
            if let Ok(value) = HeaderValue::from_str(retry_seconds) {
                response.headers_mut().insert("retry-after", value);
            }
        }

        response
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Internal(err)
    }
}

fn get_trace_id() -> String {
    let ts = uuid::Timestamp::now(uuid::NoContext);
    Uuid::new_v7(ts).to_string()
}

fn is_development_env() -> bool {
    ENVIRONMENT
        .get()
        .map(|v| v == "development")
        .unwrap_or_else(|| std::env::var("DUSKCUE_ENVIRONMENT").map(|v| v == "development").unwrap_or(false))
}

fn auth_error_to_http(err: &crate::domains::auth::AuthError) -> (StatusCode, &'static str, String) {
    use crate::domains::auth::AuthError;
    use axum::http::StatusCode;

    match err {
        AuthError::PasskeyNotFound => (StatusCode::UNAUTHORIZED, "AUTH_001", "Passkey not found".into()),
        AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "AUTH_002", "Invalid passkey signature".into()),
        AuthError::TotpFailed => (StatusCode::UNAUTHORIZED, "AUTH_003", "TOTP verification failed".into()),
        AuthError::AccountLocked { .. } => (StatusCode::FORBIDDEN, "AUTH_004", "Account locked".into()),
        AuthError::SessionExpired => (StatusCode::UNAUTHORIZED, "AUTH_005", "Session expired".into()),
        AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "AUTH_006", "Invalid credentials".into()),
        AuthError::InsufficientCapabilities { .. } => (StatusCode::FORBIDDEN, "AUTH_007", "Insufficient capabilities".into()),
        AuthError::ApiKeyInvalid => (StatusCode::UNAUTHORIZED, "AUTH_008", "API key invalid or revoked".into()),
        AuthError::InviteCodeInvalid => (StatusCode::UNAUTHORIZED, "AUTH_009", "Invite code invalid or expired".into()),
        AuthError::InviteCodeRevoked => (StatusCode::UNAUTHORIZED, "AUTH_010", "Invite code revoked".into()),
        AuthError::InviteCodeUseLimitExceeded => (StatusCode::UNAUTHORIZED, "AUTH_011", "Invite code use limit exceeded".into()),
        AuthError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "AUTH_012", "Too many failed attempts".into()),
        AuthError::DeviceLinkingExpired => (StatusCode::BAD_REQUEST, "AUTH_013", "Device linking code expired".into()),
        AuthError::DeviceLinkingDenied => (StatusCode::BAD_REQUEST, "AUTH_014", "Device linking denied by user".into()),
        AuthError::DeviceLinkingPending => (StatusCode::BAD_REQUEST, "AUTH_013", "Authorization pending".into()),
        AuthError::DeviceLinkingSlowDown => (StatusCode::BAD_REQUEST, "AUTH_013", "Slow down".into()),
        AuthError::ReauthCodeInvalid => (StatusCode::UNAUTHORIZED, "AUTH_015", "Re-authentication code invalid or expired".into()),
        AuthError::ReauthRateLimited => (StatusCode::TOO_MANY_REQUESTS, "AUTH_016", "Too many re-auth code requests".into()),
        AuthError::SetupAlreadyComplete => (StatusCode::CONFLICT, "AUTH_017", "Setup already complete".into()),
        AuthError::SetupRequired => (StatusCode::SERVICE_UNAVAILABLE, "AUTH_018", "Setup required".into()),
        AuthError::WebauthnChallengeExpired => (StatusCode::UNAUTHORIZED, "AUTH_019", "WebAuthn challenge expired".into()),
        AuthError::WebauthnRegistrationFailed { .. } => (StatusCode::BAD_REQUEST, "AUTH_020", "WebAuthn registration failed".into()),
        AuthError::WebauthnAuthenticationFailed { .. } => (StatusCode::UNAUTHORIZED, "AUTH_021", "WebAuthn authentication failed".into()),
        AuthError::PasswordTooWeak => (StatusCode::UNPROCESSABLE_ENTITY, "AUTH_022", "Password does not meet requirements".into()),
        AuthError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}
