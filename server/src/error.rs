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

    #[error(transparent)]
    Users(#[from] crate::domains::users::UsersError),

    #[error(transparent)]
    Library(#[from] crate::domains::libraries::LibrariesError),

    #[error(transparent)]
    Media(#[from] crate::domains::media::MediaError),

    #[error(transparent)]
    System(#[from] crate::domains::system::SystemError),

    #[error(transparent)]
    Playback(#[from] crate::domains::playback::PlaybackError),

    #[error(transparent)]
    Quality(#[from] crate::domains::quality::QualityError),

    #[error(transparent)]
    Subtitle(#[from] crate::domains::subtitles::SubtitleError),

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

fn users_error_to_http(err: &crate::domains::users::UsersError) -> (StatusCode, &'static str, String) {
    use crate::domains::users::UsersError;
    use axum::http::StatusCode;

    match err {
        UsersError::NotFound => (StatusCode::NOT_FOUND, "USER_001", "User not found".into()),
        UsersError::OwnerImmutable => (StatusCode::FORBIDDEN, "USER_002", "Owner account cannot be modified".into()),
        UsersError::OwnerCannotBeDeleted => (StatusCode::FORBIDDEN, "USER_003", "Owner account cannot be deleted".into()),
        UsersError::UsernameTaken => (StatusCode::CONFLICT, "USER_004", "Username already taken".into()),
        UsersError::EmailTaken => (StatusCode::CONFLICT, "USER_005", "Email already taken".into()),
        UsersError::InvalidRole(r) => (StatusCode::BAD_REQUEST, "USER_006", format!("Invalid role: {}", r)),
        UsersError::InvalidStatus(s) => (StatusCode::BAD_REQUEST, "USER_007", format!("Invalid status: {}", s)),
        UsersError::CannotModifySelf => (StatusCode::FORBIDDEN, "USER_008", "Cannot modify own account role or status".into()),
        UsersError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn library_error_to_http(err: &crate::domains::libraries::LibrariesError) -> (StatusCode, &'static str, String) {
    use crate::domains::libraries::LibrariesError;
    use axum::http::StatusCode;

    match err {
        LibrariesError::NotFound => (StatusCode::NOT_FOUND, "LIB_001", "Library not found".into()),
        LibrariesError::NameExists(n) => (StatusCode::CONFLICT, "LIB_002", format!("Library name already exists: {}", n)),
        LibrariesError::ScanInProgress => (StatusCode::CONFLICT, "LIB_003", "Library scan already in progress".into()),
        LibrariesError::RootPathNotFound(p) => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_004", format!("Root path does not exist: {}", p)),
        LibrariesError::CannotDeleteWithMedia => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_005", "Cannot delete library with existing media items".into()),
        LibrariesError::ScanAlreadyInProgress => (StatusCode::CONFLICT, "LIB_006", "Scan already in progress for this library".into()),
        LibrariesError::FilesystemWatcherFailed => (StatusCode::SERVICE_UNAVAILABLE, "LIB_007", "Filesystem watcher failed to start".into()),
        LibrariesError::MediaMatchInvalid => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_008", ".media-match file is invalid or unreadable".into()),
        LibrariesError::NfoInvalid => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_009", "NFO file is invalid or contains no usable provider IDs".into()),
        LibrariesError::ProviderIdTagMalformed(t) => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_010", format!("Provider ID tag malformed: {}", t)),
        LibrariesError::TmdbUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "LIB_011", "TMDB metadata provider unavailable".into()),
        LibrariesError::TvdbAuthFailed => (StatusCode::UNAUTHORIZED, "LIB_012", "TVDB authentication failure".into()),
        LibrariesError::ProviderRateLimited => (StatusCode::TOO_MANY_REQUESTS, "LIB_013", "Metadata provider rate limit exceeded".into()),
        LibrariesError::ProviderResponseInvalid => (StatusCode::BAD_GATEWAY, "LIB_014", "Metadata provider response validation failure".into()),
        LibrariesError::PathNotFound => (StatusCode::NOT_FOUND, "LIB_015", "Library path not found".into()),
        LibrariesError::PathExists(p) => (StatusCode::CONFLICT, "LIB_016", format!("Path already exists for this library: {}", p)),
        LibrariesError::CannotDeleteDefaultPath => (StatusCode::UNPROCESSABLE_ENTITY, "LIB_017", "Cannot delete the default library path".into()),
        LibrariesError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn media_error_to_http(err: &crate::domains::media::MediaError) -> (StatusCode, &'static str, String) {
    use crate::domains::media::MediaError;
    use axum::http::StatusCode;

    match err {
        MediaError::NotFound => (StatusCode::NOT_FOUND, "MEDIA_001", "Media item not found".into()),
        MediaError::FileNotFound => (StatusCode::NOT_FOUND, "MEDIA_002", "Media file not found".into()),
        MediaError::FileUnhealthy(r) => (StatusCode::UNPROCESSABLE_ENTITY, "MEDIA_003", format!("Media file is unhealthy: {}", r)),
        MediaError::ArtworkNotFound => (StatusCode::NOT_FOUND, "MEDIA_004", "Artwork not found".into()),
        MediaError::AlreadyExists => (StatusCode::CONFLICT, "MEDIA_006", "Media item already exists in library".into()),
        MediaError::StoryboardNotFound => (StatusCode::NOT_FOUND, "MEDIA_007", "Storyboard not found".into()),
        MediaError::InvalidMediaType(t) => (StatusCode::BAD_REQUEST, "MEDIA_001", format!("Invalid media type: {}", t)),
        MediaError::InvalidMatchState(s) => (StatusCode::BAD_REQUEST, "MEDIA_001", format!("Invalid match state: {}", s)),
        MediaError::InvalidIdentificationSource(s) => (StatusCode::BAD_REQUEST, "MEDIA_001", format!("Invalid identification source: {}", s)),
        MediaError::SeriesNotFound => (StatusCode::NOT_FOUND, "MEDIA_001", "Series not found".into()),
        MediaError::SeasonNotFound => (StatusCode::NOT_FOUND, "MEDIA_001", "Season not found".into()),
        MediaError::DuplicateSeasonNumber(n) => (StatusCode::CONFLICT, "MEDIA_006", format!("Duplicate season number {} for series", n)),
        MediaError::DuplicateEpisodeNumber(n) => (StatusCode::CONFLICT, "MEDIA_006", format!("Duplicate episode number {} for season", n)),
        MediaError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn system_error_to_http(err: &crate::domains::system::SystemError) -> (StatusCode, &'static str, String) {
    use crate::domains::system::SystemError;
    use axum::http::StatusCode;

    match err {
        SystemError::InvalidProvider(p) => (StatusCode::BAD_REQUEST, "SYS_013", format!("Invalid provider: {}", p)),
        SystemError::MissingCredential(msg) => (StatusCode::BAD_REQUEST, "SYS_014", format!("Missing credential: {}", msg)),
        SystemError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn playback_error_to_http(err: &crate::domains::playback::PlaybackError) -> (StatusCode, &'static str, String) {
    use crate::domains::playback::PlaybackError;
    use axum::http::StatusCode;

    match err {
        PlaybackError::MediaNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Media item not found".into()),
        PlaybackError::AccessDenied => (StatusCode::FORBIDDEN, "PLAY_002", "User lacks library access or play_media capability".into()),
        PlaybackError::TranscodeCapacityReached => (StatusCode::SERVICE_UNAVAILABLE, "PLAY_003", "Transcode capacity reached".into()),
        PlaybackError::FfmpegFailed(r) => (StatusCode::INTERNAL_SERVER_ERROR, "PLAY_004", format!("FFmpeg process failed: {}", r)),
        PlaybackError::SessionAlreadyActive => (StatusCode::CONFLICT, "PLAY_005", "Session already active for this item".into()),
        PlaybackError::InvalidSeekPosition(r) => (StatusCode::BAD_REQUEST, "PLAY_006", format!("Invalid seek position: {}", r)),
        PlaybackError::InvalidByteRange(r) => (StatusCode::RANGE_NOT_SATISFIABLE, "PLAY_007", format!("Invalid byte range: {}", r)),
        PlaybackError::HwAccelFallback(r) => (StatusCode::INTERNAL_SERVER_ERROR, "PLAY_008", format!("HW accel fallback: {}", r)),
        PlaybackError::FfmpegCrashed => (StatusCode::INTERNAL_SERVER_ERROR, "PLAY_009", "FFmpeg process crashed during transcode".into()),
        PlaybackError::DiskSpaceExhausted => (StatusCode::INSUFFICIENT_STORAGE, "PLAY_010", "Transcode disk space exhausted".into()),
        PlaybackError::IpBlocked => (StatusCode::FORBIDDEN, "PLAY_011", "Client IP address blocked by streaming policy".into()),
        PlaybackError::StreamLimitExceeded => (StatusCode::TOO_MANY_REQUESTS, "PLAY_012", "Per-user stream limit exceeded".into()),
        PlaybackError::TranscodeRestrictedByPolicy => (StatusCode::FORBIDDEN, "PLAY_013", "Resolution requires direct play — transcode restricted by policy".into()),
        PlaybackError::SessionNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Session not found".into()),
        PlaybackError::FileNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Media file not found".into()),
        PlaybackError::FileUnhealthy(r) => (StatusCode::UNPROCESSABLE_ENTITY, "PLAY_001", format!("Media file is unhealthy: {}", r)),
        PlaybackError::PolicyNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Streaming policy not found".into()),
        PlaybackError::PolicyNameExists(n) => (StatusCode::CONFLICT, "PLAY_014", format!("Policy name already exists: {}", n)),
        PlaybackError::SystemPolicyCannotBeDeleted => (StatusCode::FORBIDDEN, "PLAY_015", "System policy cannot be deleted".into()),
        PlaybackError::CannotRemoveDefaultPolicy => (StatusCode::FORBIDDEN, "PLAY_016", "Cannot remove default policy without assigning a replacement".into()),
        PlaybackError::InvalidResolution(r) => (StatusCode::BAD_REQUEST, "PLAY_017", format!("Invalid transcode resolution: {}", r)),
        PlaybackError::InvalidIpRange(r) => (StatusCode::BAD_REQUEST, "PLAY_018", format!("Invalid IP range: {}", r)),
        PlaybackError::InvalidStreamDecision(d) => (StatusCode::BAD_REQUEST, "PLAY_001", format!("Invalid stream decision: {}", d)),
        PlaybackError::UserItemDataNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "User item data not found".into()),
        PlaybackError::BookmarkNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Bookmark not found".into()),
        PlaybackError::PlaylistNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Playlist not found".into()),
        PlaybackError::PlaylistItemNotFound => (StatusCode::NOT_FOUND, "PLAY_001", "Playlist item not found".into()),
        PlaybackError::InvalidVisibility(v) => (StatusCode::BAD_REQUEST, "PLAY_001", format!("Invalid visibility: {}", v)),
        PlaybackError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn quality_error_to_http(err: &crate::domains::quality::QualityError) -> (StatusCode, &'static str, String) {
    use crate::domains::quality::QualityError;
    use axum::http::StatusCode;

    match err {
        QualityError::WizardTestNotFound => (StatusCode::NOT_FOUND, "QUALITY_001", "Capability wizard test not found".into()),
        QualityError::WizardAlreadyCompleted => (StatusCode::CONFLICT, "QUALITY_002", "Capability wizard already completed for this device".into()),
        QualityError::InvalidTelemetry(r) => (StatusCode::BAD_REQUEST, "QUALITY_003", format!("Invalid telemetry report: {}", r)),
        QualityError::TelemetryRateLimited => (StatusCode::TOO_MANY_REQUESTS, "QUALITY_004", "Too many telemetry reports".into()),
        QualityError::InvalidProbeResult(r) => (StatusCode::BAD_REQUEST, "QUALITY_005", format!("Invalid bandwidth probe result: {}", r)),
        QualityError::DeviceProfileNotFound => (StatusCode::NOT_FOUND, "QUALITY_006", "Device profile not found".into()),
        QualityError::TranscodeDecisionConflict => (StatusCode::CONFLICT, "QUALITY_007", "Transcode decision conflict".into()),
        QualityError::SubtitleBurnInRequired => (StatusCode::OK, "QUALITY_008", "Subtitle burn-in required".into()),
        QualityError::UnsupportedToneMappingAlgorithm(a) => (StatusCode::BAD_REQUEST, "QUALITY_009", format!("Unsupported tone mapping algorithm: {}", a)),
        QualityError::ToneMappingUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "QUALITY_010", "Tone mapping unavailable".into()),
        QualityError::InvalidQualityMode(m) => (StatusCode::BAD_REQUEST, "QUALITY_011", format!("Invalid quality mode: {}", m)),
        QualityError::MediaVersionNotFound => (StatusCode::NOT_FOUND, "QUALITY_012", "Requested media version not found".into()),
        QualityError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}

fn subtitle_error_to_http(err: &crate::domains::subtitles::SubtitleError) -> (StatusCode, &'static str, String) {
    use crate::domains::subtitles::SubtitleError;
    use axum::http::StatusCode;

    match err {
        SubtitleError::FileNotFound { .. } => (StatusCode::NOT_FOUND, "SUB_001", "Subtitle file not found".into()),
        SubtitleError::OcrUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "SUB_002", "OCR engine unavailable".into()),
        SubtitleError::OcrLowConfidence { confidence, threshold } => (StatusCode::UNPROCESSABLE_ENTITY, "SUB_003", format!("OCR confidence {} below threshold {}", confidence, threshold)),
        SubtitleError::ProviderUnavailable { provider } => (StatusCode::SERVICE_UNAVAILABLE, "SUB_004", format!("Subtitle provider unavailable: {}", provider)),
        SubtitleError::ProviderRateLimited { provider } => (StatusCode::TOO_MANY_REQUESTS, "SUB_005", format!("Subtitle provider rate limited: {}", provider)),
        SubtitleError::VoiceAnalysisFailed { reason } => (StatusCode::UNPROCESSABLE_ENTITY, "SUB_006", format!("Voice activity analysis failed: {}", reason)),
        SubtitleError::MediaItemNotFound { .. } => (StatusCode::NOT_FOUND, "SUB_001", "Media item not found".into()),
        SubtitleError::InvalidSubtitleFormat(f) => (StatusCode::BAD_REQUEST, "SUB_001", format!("Invalid subtitle format: {}", f)),
        SubtitleError::InvalidLanguageCode(c) => (StatusCode::BAD_REQUEST, "SUB_001", format!("Invalid language code: {}", c)),
        SubtitleError::FetchFailed { reason } => (StatusCode::SERVICE_UNAVAILABLE, "SUB_004", format!("Subtitle fetch failed: {}", reason)),
        SubtitleError::ConversionFailed { reason } => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", format!("Subtitle conversion failed: {}", reason)),
        SubtitleError::SyncDataNotFound { .. } => (StatusCode::NOT_FOUND, "SUB_001", "Subtitle sync data not found".into()),
        SubtitleError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error".into()),
    }
}
