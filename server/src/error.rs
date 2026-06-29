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

use axum::Json;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
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
    Validation {
        errors: Vec<FieldError>,
        instance: Option<String>,
    },

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
    Analytics(#[from] crate::domains::analytics::AnalyticsError),

    #[error(transparent)]
    Auth(#[from] crate::domains::auth::AuthError),

    #[error(transparent)]
    Backup(#[from] crate::domains::backup::BackupError),

    #[error(transparent)]
    Users(#[from] crate::domains::users::UsersError),

    #[error(transparent)]
    Library(#[from] crate::domains::libraries::LibrariesError),

    #[error(transparent)]
    Media(#[from] crate::domains::media::MediaError),

    #[error(transparent)]
    Notifications(#[from] crate::domains::notifications::NotificationsError),

    #[error(transparent)]
    Overlay(#[from] crate::domains::overlays::OverlayError),

    #[error(transparent)]
    Collections(#[from] crate::domains::collections::CollectionsError),

    #[error(transparent)]
    System(#[from] crate::domains::system::SystemError),

    #[error(transparent)]
    Playback(#[from] crate::domains::playback::PlaybackError),

    #[error(transparent)]
    Quality(#[from] crate::domains::quality::QualityError),

    #[error(transparent)]
    Subtitle(#[from] crate::domains::subtitles::SubtitleError),

    #[error(transparent)]
    Segment(#[from] crate::domains::segments::SegmentError),

    #[error(transparent)]
    Storyboard(#[from] crate::domains::storyboards::StoryboardError),

    #[error(transparent)]
    Trakt(#[from] crate::domains::trakt::TraktError),

    #[error("internal server error")]
    Internal(#[source] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, detail): (StatusCode, &str, Cow<'_, str>) = match &self {
            AppError::Analytics(e) => {
                let (s, c, d) = analytics_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Auth(e) => {
                let (s, c, d) = auth_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Backup(e) => {
                let (s, c, d) = backup_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Users(e) => {
                let (s, c, d) = users_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Library(e) => {
                let (s, c, d) = library_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Media(e) => {
                let (s, c, d) = media_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Notifications(e) => {
                let (s, c, d) = notifications_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Overlay(e) => {
                let (s, c, d) = overlay_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Collections(e) => {
                let (s, c, d) = collections_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::System(e) => {
                let (s, c, d) = system_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Playback(e) => {
                let (s, c, d) = playback_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Quality(e) => {
                let (s, c, d) = quality_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Subtitle(e) => {
                let (s, c, d) = subtitle_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Segment(e) => {
                let (s, c, d) = segment_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Storyboard(e) => {
                let (s, c, d) = storyboard_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::Trakt(e) => {
                let (s, c, d) = trakt_error_to_http(e);
                (s, c, Cow::Owned(d))
            }
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::Validation { .. } => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "VALID_001",
                Cow::Borrowed("One or more fields failed validation"),
            ),
            AppError::RateLimited { code } => (
                StatusCode::TOO_MANY_REQUESTS,
                code.as_str(),
                Cow::Borrowed("Rate limit exceeded"),
            ),
            AppError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                "FORBIDDEN",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::UnprocessableEntity(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "UNPROCESSABLE_ENTITY",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::ServiceUnavailable(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "SERVICE_UNAVAILABLE",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::GatewayTimeout(msg) => (
                StatusCode::GATEWAY_TIMEOUT,
                "GATEWAY_TIMEOUT",
                Cow::Borrowed(msg.as_str()),
            ),
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                Cow::Borrowed("Internal server error"),
            ),
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
                | AppError::Trakt(crate::domains::trakt::TraktError::ServiceUnavailable)
                | AppError::Trakt(crate::domains::trakt::TraktError::Timeout)
        );

        if wants_retry_after {
            let retry_seconds = match &self {
                AppError::RateLimited { .. } => "0",
                AppError::GatewayTimeout(_) => "30",
                AppError::ServiceUnavailable(_) => "60",
                AppError::Trakt(crate::domains::trakt::TraktError::Timeout) => "30",
                AppError::Trakt(crate::domains::trakt::TraktError::ServiceUnavailable) => "60",
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
        .unwrap_or_else(|| {
            std::env::var("DUSKCUE_ENVIRONMENT")
                .map(|v| v == "development")
                .unwrap_or(false)
        })
}

fn analytics_error_to_http(
    err: &crate::domains::analytics::AnalyticsError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::analytics::AnalyticsError;
    use axum::http::StatusCode;

    match err {
        AnalyticsError::UserNotFound => {
            (StatusCode::NOT_FOUND, "USER_001", "User not found".into())
        }
        AnalyticsError::TrustEventNotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Trust event not found".into(),
        ),
        AnalyticsError::InvalidDateRange(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid date range: {}", msg),
        ),
        AnalyticsError::InvalidTimePreset(p) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid time preset: {}", p),
        ),
        AnalyticsError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn trakt_error_to_http(
    err: &crate::domains::trakt::TraktError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::trakt::TraktError;
    use axum::http::StatusCode;

    match err {
        TraktError::AccountNotLinked => (
            StatusCode::CONFLICT,
            "TRAKT_001",
            "Trakt account not linked".into(),
        ),
        TraktError::RateLimited { .. } => (
            StatusCode::TOO_MANY_REQUESTS,
            "TRAKT_002",
            "Trakt API rate limited".into(),
        ),
        TraktError::TokenExpired => (
            StatusCode::CONFLICT,
            "TRAKT_003",
            "Trakt token expired — re-link required".into(),
        ),
        TraktError::ServiceUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "TRAKT_004",
            "Trakt API unavailable".into(),
        ),
        TraktError::Timeout => (
            StatusCode::GATEWAY_TIMEOUT,
            "TRAKT_005",
            "Trakt API timeout".into(),
        ),
        TraktError::DeviceCodeExpired => (
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "Device code expired".into(),
        ),
        TraktError::DeviceCodePending => (
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            "Device authorization pending".into(),
        ),
        TraktError::DeviceCodeDenied => (
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
            "Device authorization denied".into(),
        ),
        TraktError::SyncInProgress => (
            StatusCode::CONFLICT,
            "CONFLICT",
            "A sync is already in progress".into(),
        ),
        TraktError::NotConfigured => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Trakt integration not configured".into(),
        ),
        TraktError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn auth_error_to_http(err: &crate::domains::auth::AuthError) -> (StatusCode, &'static str, String) {
    use crate::domains::auth::AuthError;
    use axum::http::StatusCode;

    match err {
        AuthError::PasskeyNotFound => (
            StatusCode::UNAUTHORIZED,
            "AUTH_001",
            "Passkey not found".into(),
        ),
        AuthError::InvalidSignature => (
            StatusCode::UNAUTHORIZED,
            "AUTH_002",
            "Invalid passkey signature".into(),
        ),
        AuthError::TotpFailed => (
            StatusCode::UNAUTHORIZED,
            "AUTH_003",
            "TOTP verification failed".into(),
        ),
        AuthError::AccountLocked { .. } => {
            (StatusCode::FORBIDDEN, "AUTH_004", "Account locked".into())
        }
        AuthError::SessionExpired => (
            StatusCode::UNAUTHORIZED,
            "AUTH_005",
            "Session expired".into(),
        ),
        AuthError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            "AUTH_006",
            "Invalid credentials".into(),
        ),
        AuthError::InsufficientCapabilities { .. } => (
            StatusCode::FORBIDDEN,
            "AUTH_007",
            "Insufficient capabilities".into(),
        ),
        AuthError::ApiKeyInvalid => (
            StatusCode::UNAUTHORIZED,
            "AUTH_008",
            "API key invalid or revoked".into(),
        ),
        AuthError::InviteCodeInvalid => (
            StatusCode::UNAUTHORIZED,
            "AUTH_009",
            "Invite code invalid or expired".into(),
        ),
        AuthError::InviteCodeRevoked => (
            StatusCode::UNAUTHORIZED,
            "AUTH_010",
            "Invite code revoked".into(),
        ),
        AuthError::InviteCodeUseLimitExceeded => (
            StatusCode::UNAUTHORIZED,
            "AUTH_011",
            "Invite code use limit exceeded".into(),
        ),
        AuthError::RateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "AUTH_012",
            "Too many failed attempts".into(),
        ),
        AuthError::DeviceLinkingExpired => (
            StatusCode::BAD_REQUEST,
            "AUTH_013",
            "Device linking code expired".into(),
        ),
        AuthError::DeviceLinkingDenied => (
            StatusCode::BAD_REQUEST,
            "AUTH_014",
            "Device linking denied by user".into(),
        ),
        AuthError::DeviceLinkingPending => (
            StatusCode::BAD_REQUEST,
            "AUTH_013",
            "Authorization pending".into(),
        ),
        AuthError::DeviceLinkingSlowDown => {
            (StatusCode::BAD_REQUEST, "AUTH_013", "Slow down".into())
        }
        AuthError::ReauthCodeInvalid => (
            StatusCode::UNAUTHORIZED,
            "AUTH_015",
            "Re-authentication code invalid or expired".into(),
        ),
        AuthError::ReauthRateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "AUTH_016",
            "Too many re-auth code requests".into(),
        ),
        AuthError::SetupAlreadyComplete => (
            StatusCode::CONFLICT,
            "AUTH_017",
            "Setup already complete".into(),
        ),
        AuthError::SetupRequired => (
            StatusCode::SERVICE_UNAVAILABLE,
            "AUTH_018",
            "Setup required".into(),
        ),
        AuthError::WebauthnChallengeExpired => (
            StatusCode::UNAUTHORIZED,
            "AUTH_019",
            "WebAuthn challenge expired".into(),
        ),
        AuthError::WebauthnRegistrationFailed { .. } => (
            StatusCode::BAD_REQUEST,
            "AUTH_020",
            "WebAuthn registration failed".into(),
        ),
        AuthError::WebauthnAuthenticationFailed { .. } => (
            StatusCode::UNAUTHORIZED,
            "AUTH_021",
            "WebAuthn authentication failed".into(),
        ),
        AuthError::PasswordTooWeak => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "AUTH_022",
            "Password does not meet requirements".into(),
        ),
        AuthError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn backup_error_to_http(
    err: &crate::domains::backup::BackupError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::backup::BackupError;

    match err {
        BackupError::InvalidConfig(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
        BackupError::OperationInProgress => (
            StatusCode::CONFLICT,
            "SYS_007",
            "Backup already in progress".to_string(),
        ),
        BackupError::CommandUnavailable { tool, .. } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            format!("Backup command is unavailable: {tool}"),
        ),
        BackupError::CommandTimeout {
            tool,
            timeout_seconds,
        } => (
            StatusCode::GATEWAY_TIMEOUT,
            "GATEWAY_TIMEOUT",
            format!("Backup command timed out: {tool} after {timeout_seconds}s"),
        ),
        BackupError::CommandFailed { tool, stderr, .. } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SYS_009",
            format!("Backup command failed: {tool}: {stderr}"),
        ),
        BackupError::VerificationFailed(msg) => {
            (StatusCode::SERVICE_UNAVAILABLE, "SYS_009", msg.clone())
        }
        BackupError::Io(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Backup storage I/O error".to_string(),
        ),
        BackupError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Database error".to_string(),
        ),
    }
}

fn users_error_to_http(
    err: &crate::domains::users::UsersError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::users::UsersError;
    use axum::http::StatusCode;

    match err {
        UsersError::NotFound => (StatusCode::NOT_FOUND, "USER_001", "User not found".into()),
        UsersError::OwnerImmutable => (
            StatusCode::FORBIDDEN,
            "USER_002",
            "Owner account cannot be modified".into(),
        ),
        UsersError::OwnerCannotBeDeleted => (
            StatusCode::FORBIDDEN,
            "USER_003",
            "Owner account cannot be deleted".into(),
        ),
        UsersError::UsernameTaken => (
            StatusCode::CONFLICT,
            "USER_004",
            "Username already taken".into(),
        ),
        UsersError::EmailTaken => (
            StatusCode::CONFLICT,
            "USER_005",
            "Email already taken".into(),
        ),
        UsersError::InvalidRole(r) => (
            StatusCode::BAD_REQUEST,
            "USER_006",
            format!("Invalid role: {}", r),
        ),
        UsersError::InvalidStatus(s) => (
            StatusCode::BAD_REQUEST,
            "USER_007",
            format!("Invalid status: {}", s),
        ),
        UsersError::CannotModifySelf => (
            StatusCode::FORBIDDEN,
            "USER_008",
            "Cannot modify own account role or status".into(),
        ),
        UsersError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn library_error_to_http(
    err: &crate::domains::libraries::LibrariesError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::libraries::LibrariesError;
    use axum::http::StatusCode;

    match err {
        LibrariesError::NotFound => (StatusCode::NOT_FOUND, "LIB_001", "Library not found".into()),
        LibrariesError::NameExists(n) => (
            StatusCode::CONFLICT,
            "LIB_002",
            format!("Library name already exists: {}", n),
        ),
        LibrariesError::ScanInProgress => (
            StatusCode::CONFLICT,
            "LIB_003",
            "Library scan already in progress".into(),
        ),
        LibrariesError::RootPathNotFound(p) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_004",
            format!("Root path does not exist: {}", p),
        ),
        LibrariesError::CannotDeleteWithMedia => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_005",
            "Cannot delete library with existing media items".into(),
        ),
        LibrariesError::ScanAlreadyInProgress => (
            StatusCode::CONFLICT,
            "LIB_006",
            "Scan already in progress for this library".into(),
        ),
        LibrariesError::FilesystemWatcherFailed => (
            StatusCode::SERVICE_UNAVAILABLE,
            "LIB_007",
            "Filesystem watcher failed to start".into(),
        ),
        LibrariesError::MediaMatchInvalid => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_008",
            ".media-match file is invalid or unreadable".into(),
        ),
        LibrariesError::NfoInvalid => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_009",
            "NFO file is invalid or contains no usable provider IDs".into(),
        ),
        LibrariesError::ProviderIdTagMalformed(t) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_010",
            format!("Provider ID tag malformed: {}", t),
        ),
        LibrariesError::TmdbUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "LIB_011",
            "TMDB metadata provider unavailable".into(),
        ),
        LibrariesError::TvdbAuthFailed => (
            StatusCode::UNAUTHORIZED,
            "LIB_012",
            "TVDB authentication failure".into(),
        ),
        LibrariesError::ProviderRateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "LIB_013",
            "Metadata provider rate limit exceeded".into(),
        ),
        LibrariesError::ProviderResponseInvalid => (
            StatusCode::BAD_GATEWAY,
            "LIB_014",
            "Metadata provider response validation failure".into(),
        ),
        LibrariesError::PathNotFound => (
            StatusCode::NOT_FOUND,
            "LIB_015",
            "Library path not found".into(),
        ),
        LibrariesError::PathExists(p) => (
            StatusCode::CONFLICT,
            "LIB_016",
            format!("Path already exists for this library: {}", p),
        ),
        LibrariesError::CannotDeleteDefaultPath => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "LIB_017",
            "Cannot delete the default library path".into(),
        ),
        LibrariesError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn media_error_to_http(
    err: &crate::domains::media::MediaError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::media::MediaError;
    use axum::http::StatusCode;

    match err {
        MediaError::NotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Media item not found".into(),
        ),
        MediaError::FileNotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_002",
            "Media file not found".into(),
        ),
        MediaError::FileUnhealthy(r) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "MEDIA_003",
            format!("Media file is unhealthy: {}", r),
        ),
        MediaError::ArtworkNotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_004",
            "Artwork not found".into(),
        ),
        MediaError::AlreadyExists => (
            StatusCode::CONFLICT,
            "MEDIA_006",
            "Media item already exists in library".into(),
        ),
        MediaError::StoryboardNotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_007",
            "Storyboard not found".into(),
        ),
        MediaError::InvalidMediaType(t) => (
            StatusCode::BAD_REQUEST,
            "MEDIA_001",
            format!("Invalid media type: {}", t),
        ),
        MediaError::InvalidMatchState(s) => (
            StatusCode::BAD_REQUEST,
            "MEDIA_001",
            format!("Invalid match state: {}", s),
        ),
        MediaError::InvalidIdentificationSource(s) => (
            StatusCode::BAD_REQUEST,
            "MEDIA_001",
            format!("Invalid identification source: {}", s),
        ),
        MediaError::SeriesNotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Series not found".into(),
        ),
        MediaError::SeasonNotFound => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Season not found".into(),
        ),
        MediaError::DuplicateSeasonNumber(n) => (
            StatusCode::CONFLICT,
            "MEDIA_006",
            format!("Duplicate season number {} for series", n),
        ),
        MediaError::DuplicateEpisodeNumber(n) => (
            StatusCode::CONFLICT,
            "MEDIA_006",
            format!("Duplicate episode number {} for season", n),
        ),
        MediaError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn notifications_error_to_http(
    err: &crate::domains::notifications::NotificationsError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::notifications::NotificationsError;

    match err {
        NotificationsError::NotFound => (
            StatusCode::NOT_FOUND,
            "SYS_004",
            "Notification not found".into(),
        ),
        NotificationsError::NotificationTypeNotFound => (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            "Notification type not found".into(),
        ),
        NotificationsError::InvalidCategory(c) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid category: {c}"),
        ),
        NotificationsError::InvalidPriority(p) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid priority: {p}"),
        ),
        NotificationsError::InvalidChannelConfig(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid channel configuration: {msg}"),
        ),
        NotificationsError::PushDeviceNotFound => (
            StatusCode::NOT_FOUND,
            "SYS_004",
            "Push device not found".into(),
        ),
        NotificationsError::InvalidPushProvider(p) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid push provider: {p}"),
        ),
        NotificationsError::InvalidPushToken(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid push token: {msg}"),
        ),
        NotificationsError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn overlay_error_to_http(
    err: &crate::domains::overlays::OverlayError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::overlays::OverlayError;
    use axum::http::StatusCode;

    match err {
        OverlayError::NotFound => (
            StatusCode::NOT_FOUND,
            "OVERLAY_001",
            "Overlay definition not found".into(),
        ),
        OverlayError::InvalidConditions(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "OVERLAY_002",
            format!("Invalid overlay conditions: {}", msg),
        ),
        OverlayError::InvalidTextTemplate(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "OVERLAY_003",
            format!("Invalid text template: {}", msg),
        ),
        OverlayError::ImageFileNotFound(path) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "OVERLAY_004",
            format!("Overlay image file not found or unreadable: {}", path),
        ),
        OverlayError::ApplicationInProgress => (
            StatusCode::CONFLICT,
            "OVERLAY_005",
            "Overlay application already in progress".into(),
        ),
        OverlayError::CompositingFailed(msg) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "OVERLAY_006",
            format!("Overlay compositing failed: {}", msg),
        ),
        OverlayError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn collections_error_to_http(
    err: &crate::domains::collections::CollectionsError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::collections::CollectionsError;
    use axum::http::StatusCode;

    match err {
        CollectionsError::NotFound => (
            StatusCode::NOT_FOUND,
            "COLL_001",
            "Collection not found".into(),
        ),
        CollectionsError::NameAlreadyExists => (
            StatusCode::CONFLICT,
            "COLL_002",
            "Collection name already exists in this library".into(),
        ),
        CollectionsError::SyncInProgress => (
            StatusCode::CONFLICT,
            "COLL_003",
            "Collection sync already in progress".into(),
        ),
        CollectionsError::InvalidDynamicConfig(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "COLL_004",
            format!("Invalid dynamic collection configuration: {msg}"),
        ),
        CollectionsError::InvalidSmartFilter(msg) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "COLL_005",
            format!("Invalid smart filter syntax: {msg}"),
        ),
        CollectionsError::ExternalSourceUnavailable(source) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "COLL_006",
            format!("External builder source unavailable: {source}"),
        ),
        CollectionsError::ExternalRateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "COLL_007",
            "External API rate limit exceeded during collection sync".into(),
        ),
        CollectionsError::TemplateNotFound => (
            StatusCode::NOT_FOUND,
            "COLL_008",
            "Collection template not found".into(),
        ),
        CollectionsError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn system_error_to_http(
    err: &crate::domains::system::SystemError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::system::SystemError;
    use axum::http::StatusCode;

    match err {
        SystemError::ScheduledTaskNotFound(_) => (
            StatusCode::NOT_FOUND,
            "SYS_001",
            "Scheduled task not found".into(),
        ),
        SystemError::ScheduledTaskAlreadyRunning(_) => (
            StatusCode::CONFLICT,
            "SYS_002",
            "Scheduled task already running".into(),
        ),
        SystemError::InvalidCronExpression(expr) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "SYS_003",
            format!("Invalid cron expression: {}", expr),
        ),
        SystemError::SchedulerUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            "Scheduled task runner is not available".into(),
        ),
        SystemError::TaskExecutorUnavailable(task_type) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SERVICE_UNAVAILABLE",
            format!("Scheduled task executor is not registered: {}", task_type),
        ),
        SystemError::InvalidProvider(p) => (
            StatusCode::BAD_REQUEST,
            "SYS_013",
            format!("Invalid provider: {}", p),
        ),
        SystemError::MissingCredential(msg) => (
            StatusCode::BAD_REQUEST,
            "SYS_014",
            format!("Missing credential: {}", msg),
        ),
        SystemError::ConfigNotInitialized => (
            StatusCode::NOT_FOUND,
            "SYS_005",
            "Server config is not initialized".into(),
        ),
        SystemError::InvalidConfigKey(key) => (
            StatusCode::BAD_REQUEST,
            "BAD_REQUEST",
            format!("Invalid config key or group: {}", key),
        ),
        SystemError::InvalidConfigValue { field, message } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid config value for {}: {}", field, message),
        ),
        SystemError::ConfigSerialization(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
        SystemError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn playback_error_to_http(
    err: &crate::domains::playback::PlaybackError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::playback::PlaybackError;
    use axum::http::StatusCode;

    match err {
        PlaybackError::MediaNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Media item not found".into(),
        ),
        PlaybackError::AccessDenied => (
            StatusCode::FORBIDDEN,
            "PLAY_002",
            "User lacks library access or play_media capability".into(),
        ),
        PlaybackError::TranscodeCapacityReached => (
            StatusCode::SERVICE_UNAVAILABLE,
            "PLAY_003",
            "Transcode capacity reached".into(),
        ),
        PlaybackError::FfmpegFailed(r) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "PLAY_004",
            format!("FFmpeg process failed: {}", r),
        ),
        PlaybackError::SessionAlreadyActive => (
            StatusCode::CONFLICT,
            "PLAY_005",
            "Session already active for this item".into(),
        ),
        PlaybackError::InvalidSeekPosition(r) => (
            StatusCode::BAD_REQUEST,
            "PLAY_006",
            format!("Invalid seek position: {}", r),
        ),
        PlaybackError::InvalidByteRange(r) => (
            StatusCode::RANGE_NOT_SATISFIABLE,
            "PLAY_007",
            format!("Invalid byte range: {}", r),
        ),
        PlaybackError::HwAccelFallback(r) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "PLAY_008",
            format!("HW accel fallback: {}", r),
        ),
        PlaybackError::FfmpegCrashed => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "PLAY_009",
            "FFmpeg process crashed during transcode".into(),
        ),
        PlaybackError::DiskSpaceExhausted => (
            StatusCode::INSUFFICIENT_STORAGE,
            "PLAY_010",
            "Transcode disk space exhausted".into(),
        ),
        PlaybackError::IpBlocked => (
            StatusCode::FORBIDDEN,
            "PLAY_011",
            "Client IP address blocked by streaming policy".into(),
        ),
        PlaybackError::StreamLimitExceeded => (
            StatusCode::TOO_MANY_REQUESTS,
            "PLAY_012",
            "Per-user stream limit exceeded".into(),
        ),
        PlaybackError::TranscodeRestrictedByPolicy => (
            StatusCode::FORBIDDEN,
            "PLAY_013",
            "Resolution requires direct play — transcode restricted by policy".into(),
        ),
        PlaybackError::SessionNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Session not found".into(),
        ),
        PlaybackError::FileNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Media file not found".into(),
        ),
        PlaybackError::FileUnhealthy(r) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "PLAY_001",
            format!("Media file is unhealthy: {}", r),
        ),
        PlaybackError::PolicyNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Streaming policy not found".into(),
        ),
        PlaybackError::PolicyNameExists(n) => (
            StatusCode::CONFLICT,
            "PLAY_014",
            format!("Policy name already exists: {}", n),
        ),
        PlaybackError::SystemPolicyCannotBeDeleted => (
            StatusCode::FORBIDDEN,
            "PLAY_015",
            "System policy cannot be deleted".into(),
        ),
        PlaybackError::CannotRemoveDefaultPolicy => (
            StatusCode::FORBIDDEN,
            "PLAY_016",
            "Cannot remove default policy without assigning a replacement".into(),
        ),
        PlaybackError::InvalidResolution(r) => (
            StatusCode::BAD_REQUEST,
            "PLAY_017",
            format!("Invalid transcode resolution: {}", r),
        ),
        PlaybackError::InvalidIpRange(r) => (
            StatusCode::BAD_REQUEST,
            "PLAY_018",
            format!("Invalid IP range: {}", r),
        ),
        PlaybackError::InvalidStreamDecision(d) => (
            StatusCode::BAD_REQUEST,
            "PLAY_001",
            format!("Invalid stream decision: {}", d),
        ),
        PlaybackError::UserItemDataNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "User item data not found".into(),
        ),
        PlaybackError::BookmarkNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Bookmark not found".into(),
        ),
        PlaybackError::PlaylistNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Playlist not found".into(),
        ),
        PlaybackError::PlaylistItemNotFound => (
            StatusCode::NOT_FOUND,
            "PLAY_001",
            "Playlist item not found".into(),
        ),
        PlaybackError::InvalidVisibility(v) => (
            StatusCode::BAD_REQUEST,
            "PLAY_001",
            format!("Invalid visibility: {}", v),
        ),
        PlaybackError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn quality_error_to_http(
    err: &crate::domains::quality::QualityError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::quality::QualityError;
    use axum::http::StatusCode;

    match err {
        QualityError::WizardTestNotFound => (
            StatusCode::NOT_FOUND,
            "QUALITY_001",
            "Capability wizard test not found".into(),
        ),
        QualityError::WizardAlreadyCompleted => (
            StatusCode::CONFLICT,
            "QUALITY_002",
            "Capability wizard already completed for this device".into(),
        ),
        QualityError::InvalidTelemetry(r) => (
            StatusCode::BAD_REQUEST,
            "QUALITY_003",
            format!("Invalid telemetry report: {}", r),
        ),
        QualityError::TelemetryRateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "QUALITY_004",
            "Too many telemetry reports".into(),
        ),
        QualityError::InvalidProbeResult(r) => (
            StatusCode::BAD_REQUEST,
            "QUALITY_005",
            format!("Invalid bandwidth probe result: {}", r),
        ),
        QualityError::DeviceProfileNotFound => (
            StatusCode::NOT_FOUND,
            "QUALITY_006",
            "Device profile not found".into(),
        ),
        QualityError::TranscodeDecisionConflict => (
            StatusCode::CONFLICT,
            "QUALITY_007",
            "Transcode decision conflict".into(),
        ),
        QualityError::SubtitleBurnInRequired => (
            StatusCode::OK,
            "QUALITY_008",
            "Subtitle burn-in required".into(),
        ),
        QualityError::UnsupportedToneMappingAlgorithm(a) => (
            StatusCode::BAD_REQUEST,
            "QUALITY_009",
            format!("Unsupported tone mapping algorithm: {}", a),
        ),
        QualityError::ToneMappingUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "QUALITY_010",
            "Tone mapping unavailable".into(),
        ),
        QualityError::InvalidQualityMode(m) => (
            StatusCode::BAD_REQUEST,
            "QUALITY_011",
            format!("Invalid quality mode: {}", m),
        ),
        QualityError::MediaVersionNotFound => (
            StatusCode::NOT_FOUND,
            "QUALITY_012",
            "Requested media version not found".into(),
        ),
        QualityError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn subtitle_error_to_http(
    err: &crate::domains::subtitles::SubtitleError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::subtitles::SubtitleError;
    use axum::http::StatusCode;

    match err {
        SubtitleError::FileNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "SUB_001",
            "Subtitle file not found".into(),
        ),
        SubtitleError::OcrUnavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SUB_002",
            "OCR engine unavailable".into(),
        ),
        SubtitleError::OcrLowConfidence {
            confidence,
            threshold,
        } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "SUB_003",
            format!(
                "OCR confidence {} below threshold {}",
                confidence, threshold
            ),
        ),
        SubtitleError::ProviderUnavailable { provider } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SUB_004",
            format!("Subtitle provider unavailable: {}", provider),
        ),
        SubtitleError::ProviderRateLimited { provider } => (
            StatusCode::TOO_MANY_REQUESTS,
            "SUB_005",
            format!("Subtitle provider rate limited: {}", provider),
        ),
        SubtitleError::VoiceAnalysisFailed { reason } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "SUB_006",
            format!("Voice activity analysis failed: {}", reason),
        ),
        SubtitleError::MediaItemNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "SUB_001",
            "Media item not found".into(),
        ),
        SubtitleError::InvalidSubtitleFormat(f) => (
            StatusCode::BAD_REQUEST,
            "SUB_001",
            format!("Invalid subtitle format: {}", f),
        ),
        SubtitleError::InvalidLanguageCode(c) => (
            StatusCode::BAD_REQUEST,
            "SUB_001",
            format!("Invalid language code: {}", c),
        ),
        SubtitleError::InvalidSubtitleMode(m) => (
            StatusCode::BAD_REQUEST,
            "SUB_001",
            format!("Invalid subtitle mode: {}", m),
        ),
        SubtitleError::InvalidOcrEngine(e) => (
            StatusCode::BAD_REQUEST,
            "SUB_001",
            format!("Invalid OCR engine: {}", e),
        ),
        SubtitleError::FetchFailed { reason } => (
            StatusCode::SERVICE_UNAVAILABLE,
            "SUB_004",
            format!("Subtitle fetch failed: {}", reason),
        ),
        SubtitleError::ConversionFailed { reason } => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            format!("Subtitle conversion failed: {}", reason),
        ),
        SubtitleError::SyncDataNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "SUB_001",
            "Subtitle sync data not found".into(),
        ),
        SubtitleError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn segment_error_to_http(
    err: &crate::domains::segments::SegmentError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::segments::SegmentError;
    use axum::http::StatusCode;

    match err {
        SegmentError::MediaItemNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Media item not found".into(),
        ),
        SegmentError::SegmentNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Segment not found".into(),
        ),
        SegmentError::LibraryNotFound { .. } => {
            (StatusCode::NOT_FOUND, "LIB_001", "Library not found".into())
        }
        SegmentError::InvalidSegmentType(t) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid segment type: {}", t),
        ),
        SegmentError::InvalidSegmentSource(s) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid segment source: {}", s),
        ),
        SegmentError::InvalidTimestamps {
            start_ms,
            end_ms,
            skip_to_ms,
        } => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!(
                "Invalid timestamps: start_ms={}, end_ms={}, skip_to_ms={} (must satisfy start_ms >= 0, end_ms > start_ms, start_ms <= skip_to_ms <= end_ms)",
                start_ms, end_ms, skip_to_ms
            ),
        ),
        SegmentError::ManualSegmentExists { segment_type } => (
            StatusCode::CONFLICT,
            "CONFLICT",
            format!(
                "Manual segment already exists for type {} on this item",
                segment_type
            ),
        ),
        SegmentError::AnalysisAlreadyInProgress { .. } => (
            StatusCode::CONFLICT,
            "CONFLICT",
            "Segment analysis already in progress for this library".into(),
        ),
        SegmentError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}

fn storyboard_error_to_http(
    err: &crate::domains::storyboards::StoryboardError,
) -> (StatusCode, &'static str, String) {
    use crate::domains::storyboards::StoryboardError;
    use axum::http::StatusCode;

    match err {
        StoryboardError::MediaItemNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "MEDIA_001",
            "Media item not found".into(),
        ),
        StoryboardError::MediaFileNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "MEDIA_002",
            "Media file not found".into(),
        ),
        StoryboardError::StoryboardNotFound { .. } => (
            StatusCode::NOT_FOUND,
            "MEDIA_007",
            "Storyboard not found (not yet generated for this item)".into(),
        ),
        StoryboardError::LibraryNotFound { .. } => {
            (StatusCode::NOT_FOUND, "LIB_001", "Library not found".into())
        }
        StoryboardError::GenerationAlreadyInProgress { .. } => (
            StatusCode::CONFLICT,
            "SYS_002",
            "Storyboard generation already in progress for this library".into(),
        ),
        StoryboardError::InvalidSpriteFilename(f) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "VALID_001",
            format!("Invalid sprite filename: {}", f),
        ),
        StoryboardError::Database(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "INTERNAL",
            "Internal server error".into(),
        ),
    }
}
