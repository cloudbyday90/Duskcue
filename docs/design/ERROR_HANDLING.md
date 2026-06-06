# Error Handling Design

## Overview

This document defines the error handling strategy for the Rust server. It covers crate selection, architecture layers, API error response format, environment-aware behavior, error codes, and implementation rules.

## Crates

| Crate | Version | Role | Where Used |
|---|---|---|---|
| **thiserror** | v2 | Typed error enums per domain module | `db/error.rs`, `auth/error.rs`, `library/error.rs`, `media/error.rs`, `trakt/error.rs`, etc. |
| **anyhow** | v1 | Dynamic errors for internal services | Startup, FFmpeg orchestration, batch processing, background tasks |

### Why These Two

| Crate | Strength | Limitation | Our Use |
|---|---|---|---|
| **thiserror** | Typed enums callers can match on; `#[from]` for ergonomic `?`; minimal boilerplate | Must define variants upfront | Domain modules where handlers need to distinguish errors |
| **anyhow** | Zero-effort error propagation; `.context()` for messages; works with any error type | All errors become the same type — callers can't match | Internal code where errors are logged, not matched |

Other crates evaluated and rejected:

| Crate | Why Not |
|---|---|
| **snafu** | Richer context model but higher complexity; thiserror covers our needs with less overhead |
| **error-stack** | Powerful backtrace/context layers but heavy for a Duskcue; our `trace_id` + structured logging provides equivalent debugging |
| **eyre** | anyhow fork with better spantrace reports; minimal benefit over anyhow for our use case |

## Three-Layer Architecture

```
┌─────────────────────────────────────────────────────┐
│  Domain Layer (thiserror)                            │                                                      │
│                                                      │
│  db/error.rs      → DbError enum                    │
│  auth/error.rs    → AuthError enum                  │
│  library/error.rs → LibraryError enum               │
│  media/error.rs   → MediaError enum                 │
│  trakt/error.rs   → TraktError enum                 │
│  subtitle/error.rs → SubtitleError enum             │
│  config/error.rs  → ConfigError enum                │
│  task/error.rs    → TaskError enum                  │
│  notification/error.rs → NotificationError enum     │
│                                                      │
│  Each enum:                                          │
│    - #[derive(Error, Debug)]                         │
│    - #[from] for automatic From<T> impls            │
│    - #[source] for preserving error chains           │
│    - Custom variants with domain-specific fields     │
├─────────────────────────────────────────────────────┤
│  Application Layer (AppError)                        │                                                      │
│                                                      │
│  error/app.rs → AppError enum                        │
│    - impl IntoResponse for Axum                      │
│    - impl From<DbError>                              │
│    - impl From<AuthError>                            │
│    - impl From<LibraryError>                         │
│    - impl From<MediaError>                           │
│    - impl From<TraktError>                           │
│    - impl From<SubtitleError>                        │
│    - impl From<anyhow::Error> (→ Internal)           │
│    - Maps domain errors → HTTP status + RFC 9457    │
├─────────────────────────────────────────────────────┤
│  HTTP Layer (Axum handlers)                          │                                                      │
│                                                      │
│  fn handler() -> Result<Json<T>, AppError>           │
│    - ? converts domain errors automatically          │
│    - AppError.into_response() → RFC 9457 JSON        │
└─────────────────────────────────────────────────────┘
```

### Domain Layer Example

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("passkey not found")]
    PasskeyNotFound,

    #[error("invalid passkey signature")]
    InvalidSignature,

    #[error("TOTP verification failed")]
    TotpFailed,

    #[error("account locked until {until}")]
    AccountLocked { until: DateTime<Utc> },

    #[error("session expired")]
    SessionExpired,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("insufficient capabilities: requires {required:?}")]
    InsufficientCapabilities { required: Vec<String> },

    #[error("API key invalid or revoked")]
    ApiKeyInvalid,

    #[error("invite code invalid or expired")]
    InviteCodeInvalid,

    #[error("invite code revoked")]
    InviteCodeRevoked,

    #[error("invite code use limit exceeded")]
    InviteCodeUseLimitExceeded,

    #[error("too many failed authentication attempts")]
    RateLimited,

    #[error("device linking code expired")]
    DeviceLinkingExpired,

    #[error("device linking denied by user")]
    DeviceLinkingDenied,

    #[error("re-authentication code invalid or expired")]
    ReauthCodeInvalid,

    #[error("too many re-auth code requests")]
    ReauthRateLimited,

    #[error(transparent)]
    Database(#[from] DbError),
}
```

### SubtitleError Example

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SubtitleError {
    #[error("subtitle file not found: {id}")]
    FileNotFound { id: String },

    #[error("OCR engine unavailable (PaddleOCR and Tesseract both missing)")]
    OrcUnavailable,

    #[error("OCR confidence {confidence} below threshold {threshold}")]
    OrcLowConfidence { confidence: f64, threshold: f64 },

    #[error("subtitle provider unavailable: {provider}")]
    ProviderUnavailable { provider: String },

    #[error("subtitle provider rate limited: {provider}")]
    ProviderRateLimited { provider: String },

    #[error("voice activity analysis failed: {reason}")]
    VoiceAnalysisFailed { reason: String },

    #[error(transparent)]
    Database(#[from] DbError),
}
```

### Application Layer Example

```rust
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

#[derive(Error, Debug)]
pub enum AppError {
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Database(#[from] DbError),
    #[error(transparent)]
    Library(#[from] LibraryError),
    #[error(transparent)]
    Media(#[from] MediaError),
    #[error(transparent)]
    Trakt(#[from] TraktError),
    #[error(transparent)]
    Subtitle(#[from] SubtitleError),
    #[error(transparent)]
    Task(#[from] TaskError),
    #[error(transparent)]
    Overlay(#[from] OverlayError),
    #[error(transparent)]
    Collection(#[from] CollectionError),

    #[error("rate limit exceeded: {code}")]
    RateLimited { code: String },

    #[error("not found: {0}")]
    NotFound(String),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("internal server error")]
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, detail) = match &self {
            AppError::Auth(e) => match e {
                AuthError::PasskeyNotFound => (StatusCode::UNAUTHORIZED, "AUTH_001", "Passkey not found"),
                AuthError::InvalidSignature => (StatusCode::UNAUTHORIZED, "AUTH_002", "Invalid passkey signature"),
                AuthError::TotpFailed => (StatusCode::UNAUTHORIZED, "AUTH_003", "TOTP verification failed"),
                AuthError::AccountLocked { until } => (StatusCode::FORBIDDEN, "AUTH_004", &format!("Account locked until {}", until)),
                AuthError::SessionExpired => (StatusCode::UNAUTHORIZED, "AUTH_005", "Session expired"),
                AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "AUTH_006", "Invalid credentials"),
                AuthError::InsufficientCapabilities { .. } => (StatusCode::FORBIDDEN, "AUTH_007", "Insufficient capabilities"),
                AuthError::ApiKeyInvalid => (StatusCode::UNAUTHORIZED, "AUTH_008", "API key invalid or revoked"),
                AuthError::InviteCodeInvalid => (StatusCode::UNAUTHORIZED, "AUTH_009", "Invite code invalid or expired"),
                AuthError::InviteCodeRevoked => (StatusCode::UNAUTHORIZED, "AUTH_010", "Invite code revoked"),
                AuthError::InviteCodeUseLimitExceeded => (StatusCode::UNAUTHORIZED, "AUTH_011", "Invite code use limit exceeded"),
                AuthError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "AUTH_012", "Too many failed attempts"),
                AuthError::DeviceLinkingExpired => (StatusCode::BAD_REQUEST, "AUTH_013", "Device linking code expired"),
                AuthError::DeviceLinkingDenied => (StatusCode::BAD_REQUEST, "AUTH_014", "Device linking denied by user"),
                AuthError::ReauthCodeInvalid => (StatusCode::UNAUTHORIZED, "AUTH_015", "Re-authentication code invalid or expired"),
                AuthError::ReauthRateLimited => (StatusCode::TOO_MANY_REQUESTS, "AUTH_016", "Too many re-auth code requests"),
                AuthError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            AppError::Library(e) => match e {
                LibraryError::NotFound { .. } => (StatusCode::NOT_FOUND, "LIB_001", "Library not found"),
                LibraryError::NameExists { .. } => (StatusCode::CONFLICT, "LIB_002", "Library name already exists"),
                LibraryError::ScanInProgress { .. } => (StatusCode::CONFLICT, "LIB_003", "Library scan already in progress"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::Media(e) => match e {
                MediaError::NotFound { .. } => (StatusCode::NOT_FOUND, "MEDIA_001", "Media item not found"),
                MediaError::FileNotFound { .. } => (StatusCode::NOT_FOUND, "MEDIA_002", "Media file not found"),
                MediaError::FileUnhealthy { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "MEDIA_003", "Media file is unhealthy"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::Trakt(e) => match e {
                TraktError::AccountNotLinked => (StatusCode::CONFLICT, "TRAKT_001", "Trakt account not linked"),
                TraktError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "TRAKT_002", "Trakt API rate limited"),
                TraktError::TokenExpired => (StatusCode::CONFLICT, "TRAKT_003", "Trakt token expired"),
                TraktError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "TRAKT_004", "Trakt API unavailable"),
                TraktError::Timeout => (StatusCode::GATEWAY_TIMEOUT, "TRAKT_005", "Trakt API timeout"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::Task(e) => match e {
                TaskError::NotFound { .. } => (StatusCode::NOT_FOUND, "SYS_001", "Scheduled task not found"),
                TaskError::AlreadyRunning { .. } => (StatusCode::CONFLICT, "SYS_002", "Scheduled task already running"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::Subtitle(e) => match e {
                SubtitleError::FileNotFound { .. } => (StatusCode::NOT_FOUND, "SUB_001", "Subtitle file not found"),
                SubtitleError::OcrUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "SUB_002", "OCR engine unavailable"),
                SubtitleError::OcrLowConfidence { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "SUB_003", "OCR confidence below threshold"),
                SubtitleError::ProviderUnavailable { .. } => (StatusCode::SERVICE_UNAVAILABLE, "SUB_004", "Subtitle provider unavailable"),
                SubtitleError::ProviderRateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "SUB_005", "Subtitle provider rate limited"),
                SubtitleError::VoiceAnalysisFailed { .. } => (StatusCode::UNPROCESSABLE_ENTITY, "SUB_006", "Voice activity analysis failed"),
                _ => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
            },
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "CONFLICT", msg),
            AppError::RateLimited { code } => (StatusCode::TOO_MANY_REQUESTS, code, "Rate limit exceeded"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL", "Internal server error"),
        };

        let detail = if status.as_u16() >= 500 && !is_development_env() {
            std::borrow::Cow::Borrowed("Internal server error")
        } else {
            std::borrow::Cow::Owned(detail.to_string())
        };

        let mut body = ProblemDetail {
            r#type: format!("/errors/{}", code.to_lowercase()),
            title: code.to_string(),
            status: status.as_u16(),
            detail: detail.into_owned(),
            trace_id: get_trace_id(),
            ..Default::default()
        };

        if is_development_env() {
            if let AppError::Internal(ref err) = self {
                body = ProblemDetail {
                    debug_detail: Some(format!("{:?}", err)),
                    ..body
                };
            }
        }

        let mut response = (status, Json(body)).into_response();

        if matches!(
            &self,
            AppError::Trakt(TraktError::ServiceUnavailable) | AppError::Trakt(TraktError::Timeout)
            | AppError::Subtitle(SubtitleError::ProviderUnavailable { .. })
            | AppError::RateLimited { .. }
        ) {
            if let Ok(retry_after) = HeaderValue::from_str(
                match &self {
                    AppError::RateLimited { .. } => "0",
                    _ => "60",
                }
            ) {
                response.headers_mut().insert("retry-after", retry_after);
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
```

## API Error Response Format — RFC 9457 Problem Details

All API errors return [RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) Problem Details JSON. RFC 9457 (published 2023) obsoletes RFC 7807 and is the current Internet Standard for HTTP API error responses. The format is identical to RFC 7807 — `type`, `title`, `status`, `detail`, `instance` — with clarified semantics and a registered IANA problem type registry.

### Response Structure

```json
{
    "type": "/errors/auth_001",
    "title": "AUTH_001",
    "status": 401,
    "detail": "Passkey not found",
    "trace_id": "abc-123-def-456"
}
```

### Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | string (URI) | Yes | Identifies the error type. Machine-readable. Clients match on this or `title` |
| `title` | string | Yes | Short human-readable summary. Same for all instances of this error type. Matches the error code |
| `status` | integer | Yes | HTTP status code. Redundant with HTTP status line but useful in body |
| `detail` | string | Yes | Specific explanation of this occurrence |
| `trace_id` | string | Yes | Correlation ID for server log lookup |
| `instance` | string (URI) | No | URI of the specific resource that caused the error |
| `errors` | array | No | Per-field validation errors (only on `VALID_001`) |

### Validation Error Example (VALID_001)

```json
{
    "type": "/errors/valid_001",
    "title": "VALID_001",
    "status": 422,
    "detail": "One or more fields failed validation",
    "trace_id": "abc-123-def-456",
    "instance": "/api/v1/libraries",
    "errors": [
        {
            "field": "name",
            "code": "REQUIRED",
            "message": "Library name is required"
        },
        {
            "field": "root_path",
            "code": "PATH_NOT_FOUND",
            "message": "Directory does not exist"
        }
    ]
}
```

### HTTP Status Code Usage

| Status | When |
|---|---|
| `400 Bad Request` | Malformed request body, invalid query parameters |
| `401 Unauthorized` | Missing or invalid authentication (not logged in) |
| `403 Forbidden` | Authenticated but lacks required capabilities |
| `404 Not Found` | Resource does not exist |
| `409 Conflict` | Duplicate resource, state conflict (scan already running) |
| `422 Unprocessable Entity` | Validation failure, semantic error in request |
| `429 Too Many Requests` | Rate limit exceeded |
| `500 Internal Server Error` | Unexpected server error (logged, trace_id provided) |
| `503 Service Unavailable` | External dependency (Trakt, TMDB, OMDb) is down or unreachable |
| `504 Gateway Timeout` | External dependency did not respond in time |

### Retry-After Header

503 and 504 responses include a `Retry-After` header (in seconds) when the retry interval is known. Clients SHOULD respect this header and not retry until the interval elapses.

## Error Code Registry

`DOMAIN_NUMBER` pattern. Stable machine-readable codes that clients can depend on across API versions. New codes are appended; existing codes are never renamed or removed (only deprecated).

### AUTH — Authentication & Authorization

| Code | HTTP | Description |
|---|---|---|
| `AUTH_001` | 401 | Passkey not found |
| `AUTH_002` | 401 | Invalid passkey signature |
| `AUTH_003` | 401 | TOTP verification failed |
| `AUTH_004` | 403 | Account locked |
| `AUTH_005` | 401 | Session expired |
| `AUTH_006` | 401 | Invalid credentials |
| `AUTH_007` | 403 | Insufficient capabilities |
| `AUTH_008` | 401 | API key invalid or revoked |
| `AUTH_009` | 401 | Invite code invalid or expired |
| `AUTH_010` | 401 | Invite code revoked |
| `AUTH_011` | 401 | Invite code use limit exceeded |
| `AUTH_012` | 429 | Too many failed attempts (rate limited) |
| `AUTH_013` | 400 | Device linking code expired |
| `AUTH_014` | 400 | Device linking denied by user |
| `AUTH_015` | 401 | Re-authentication code invalid or expired |
| `AUTH_016` | 429 | Too many re-auth code requests (rate limited) |

### USER — User Management

| Code | HTTP | Description |
|---|---|---|
| `USER_001` | 404 | User not found |
| `USER_002` | 409 | Username already exists |
| `USER_003` | 409 | Email already exists |
| `USER_004` | 403 | Cannot modify owner account |
| `USER_005` | 422 | Invitation code invalid or expired |
| `USER_006` | 422 | Invitation usage limit reached |

### LIB — Libraries

| Code | HTTP | Description |
|---|---|---|
| `LIB_001` | 404 | Library not found |
| `LIB_002` | 409 | Library name already exists |
| `LIB_003` | 409 | Library scan already in progress |
| `LIB_004` | 422 | Root path does not exist |
| `LIB_005` | 422 | Cannot delete library with existing media items |
| `LIB_006` | 409 | Scan already in progress for this library |
| `LIB_007` | 503 | Filesystem watcher failed to start (see logs for fallback mode) |
| `LIB_008` | 422 | `.media-match` file is invalid or unreadable |
| `LIB_009` | 422 | NFO file is invalid or contains no usable provider IDs |
| `LIB_010` | 422 | Provider ID tag in folder/filename is malformed |
| `LIB_011` | 503 | TMDB metadata provider unavailable during enrichment |
| `LIB_012` | 401 | TVDB authentication failure (invalid API key) |
| `LIB_013` | 429 | Metadata provider rate limit exceeded |
| `LIB_014` | 502 | Metadata provider response validation failure |

### MEDIA — Media Items & Files

| Code | HTTP | Description |
|---|---|---|
| `MEDIA_001` | 404 | Media item not found |
| `MEDIA_002` | 404 | Media file not found |
| `MEDIA_003` | 422 | Media file is unhealthy |
| `MEDIA_004` | 404 | Artwork not found |
| `MEDIA_005` | 404 | Subtitle file not found (use SUB_001 instead) |
| `MEDIA_006` | 409 | Media item already exists in library |
| `MEDIA_007` | 404 | Storyboard not found (not yet generated for this item) |

### PLAY — Playback & Streaming

| Code | HTTP | Description |
|---|---|---|
| `PLAY_001` | 404 | Media item not found |
| `PLAY_002` | 403 | User lacks library access or `play_media` capability |
| `PLAY_003` | 503 | Transcode capacity reached (global max concurrent transcodes) |
| `PLAY_004` | 500 | FFmpeg process failed |
| `PLAY_005` | 409 | Session already active for this item |
| `PLAY_006` | 400 | Invalid seek position |
| `PLAY_007` | 416 | Invalid byte range for direct stream |
| `PLAY_008` | 500 | Hardware acceleration initialization failed; fell back to software |
| `PLAY_009` | 500 | FFmpeg process crashed during transcode; session terminated |
| `PLAY_010` | 507 | Transcode disk space exhausted |
| `PLAY_011` | 403 | Client IP address blocked by streaming policy |
| `PLAY_012` | 429 | Per-user stream limit exceeded (max_streams or max_transcode_streams) |
| `PLAY_013` | 403 | Resolution requires direct play — transcode restricted by policy (e.g. 4K) |

### TRAKT — Trakt.tv Integration

| Code | HTTP | Description |
|---|---|---|
| `TRAKT_001` | 409 | Trakt account not linked |
| `TRAKT_002` | 429 | Trakt API rate limited |
| `TRAKT_003` | 409 | Trakt token expired (needs re-link) |
| `TRAKT_004` | 503 | Trakt API unavailable |
| `TRAKT_005` | 504 | Trakt API timeout |

### SYS — System & Scheduled Tasks

| Code | HTTP | Description |
|---|---|---|
| `SYS_001` | 404 | Scheduled task not found |
| `SYS_002` | 409 | Scheduled task already running |
| `SYS_003` | 422 | Invalid cron expression |
| `SYS_004` | 404 | Notification not found |
| `SYS_005` | 404 | Server config not initialized |
| `SYS_006` | 503 | External service unavailable (TMDB, OMDb, etc.) |
| `SYS_007` | 409 | Backup already in progress |
| `SYS_008` | 503 | WAL-G backup failed |
| `SYS_009` | 503 | Backup verification failed |
| `SYS_010` | 503 | System CPU too high for transcode (resource limit) |
| `SYS_011` | 503 | System memory pressure — transcode rejected |
| `SYS_012` | 503 | CPU thermal throttle — transcode rejected (ARM64) |

### VALID — Validation

| Code | HTTP | Description |
|---|---|---|
| `VALID_001` | 422 | Validation error (always includes `errors` array with per-field details) |

### CLASSIFARR — No Dedicated Codes

There are no `CLASSIFARR_*` error codes. Classifarr is a passive consumer of our API — it queries our read-only endpoints and receives the same standard RFC 9457 error responses as any other API client. Our server does not call Classifarr, receive webhooks from Classifarr, or have any integration path where Classifarr-specific errors could occur on our side.

When Classifarr queries our endpoints, errors are standard:

| Classifarr Request Fails Because | Our Response |
|---|---|
| Invalid or revoked API key | `AUTH_008` (401) |
| Library not found | `LIB_001` (404) |
| Media item not found | `MEDIA_001` (404) |
| Database error | `INTERNAL` (500) with trace_id |

If Classifarr cannot reach our server, receives a timeout, or encounters a network error — that is Classifarr's error to handle, not ours. Our responsibility ends at returning correct HTTP responses to authenticated API requests.

### QUALITY — Quality Management

| Code | HTTP | Description |
|---|---|---|
| `QUALITY_001` | 400 | Capability wizard test not found |
| `QUALITY_002` | 409 | Capability wizard already completed for this device |
| `QUALITY_003` | 400 | Invalid telemetry report (malformed payload) |
| `QUALITY_004` | 429 | Too many telemetry reports (rate limited) |
| `QUALITY_005` | 400 | Invalid bandwidth probe result (malformed payload) |
| `QUALITY_006` | 404 | Device profile not found |
| `QUALITY_007` | 409 | Transcode decision conflict (concurrent request for same item) |
| `QUALITY_008` | 200 | Subtitle burn-in required (warning — PGS burn-in was necessary) |
| `QUALITY_009` | 400 | Unsupported tone mapping algorithm |
| `QUALITY_010` | 503 | Tone mapping unavailable (no supported algorithm for hardware) |
| `QUALITY_011` | 400 | Invalid quality mode selection |
| `QUALITY_012` | 404 | Requested media version not found |

### SUB — Subtitles

| Code | HTTP | Description |
|---|---|---|
| `SUB_001` | 404 | Subtitle file not found |
| `SUB_002` | 503 | OCR engine unavailable (PaddleOCR and Tesseract both missing) |
| `SUB_003` | 422 | OCR confidence below threshold (result may be inaccurate) |
| `SUB_004` | 503 | Subtitle provider unavailable (OpenSubtitles/SubDL API error) |
| `SUB_005` | 429 | Subtitle provider rate limited |
| `SUB_006` | 422 | Voice activity analysis failed (no speech detected or audio too short) |

### OVERLAY — Metadata Overlays

| Code | HTTP | Description |
|---|---|---|
| `OVERLAY_001` | 404 | Overlay definition not found |
| `OVERLAY_002` | 422 | Invalid overlay conditions (malformed JSONB filter) |
| `OVERLAY_003` | 422 | Invalid text template (unresolved variable or syntax error) |
| `OVERLAY_004` | 503 | Overlay image file not found or unreadable |
| `OVERLAY_005` | 409 | Overlay application already in progress |
| `OVERLAY_006` | 500 | Overlay compositing failed (image processing error) |

### COLL — Collections

| Code | HTTP | Description |
|---|---|---|
| `COLL_001` | 404 | Collection not found |
| `COLL_002` | 409 | Collection name already exists in this library |
| `COLL_003` | 409 | Collection sync already in progress |
| `COLL_004` | 422 | Invalid dynamic collection configuration |
| `COLL_005` | 422 | Invalid smart filter syntax |
| `COLL_006` | 503 | External builder source unavailable (TMDb, Trakt, etc.) |
| `COLL_007` | 429 | External API rate limit exceeded during collection sync |
| `COLL_008` | 404 | Collection template not found |

### RATE — HTTP-Layer Rate Limiting

Generic rate limit error returned by the governor middleware. This is distinct from domain-specific rate limit errors (AUTH_012, QUALITY_004, etc.) which are business-logic rate limits within individual domains.

| Code | HTTP | Description |
|---|---|---|
| `RATE_001` | 429 | Global rate limit exceeded (per-IP) |
| `RATE_002` | 429 | Auth endpoint rate limit exceeded (per-IP) |
| `RATE_003` | 429 | Authenticated rate limit exceeded (per-user) |
| `RATE_004` | 429 | Streaming rate limit exceeded (per-session) |
| `RATE_005` | 429 | Admin rate limit exceeded (per-user) |

All `RATE_*` responses include a `Retry-After` header with the number of seconds until the limit resets.

### MIGR — Platform Migration

| Code | HTTP | Description |
|---|---|---|
| `MIGR_001` | 404 | Migration not found |
| `MIGR_002` | 409 | Migration already in progress |
| `MIGR_003` | 502 | Source platform unreachable (Jellyfin/Emby connection failed) |
| `MIGR_004` | 422 | Invalid source configuration (bad URL, missing API key) |
| `MIGR_005` | 422 | Invalid Plex database file (not SQLite, missing tables, corrupted) |
| `MIGR_006` | 409 | User mapping conflict (same source user mapped twice) |
| `MIGR_007` | 422 | No user mappings provided (at least one required) |
| `MIGR_008` | 422 | No watch data found on source platform |
| `MIGR_009` | 413 | Plex database file too large (max 10 GB) |
| `MIGR_010` | 507 | Insufficient disk space for Plex database upload |

## Environment-Aware Error Responses

Error response detail varies by environment. The server reads the `environment` field from the `server_config.security` JSONB group at startup and uses it to determine what information to include in error responses.

### Behavior by Environment

| Concern | `development` | `staging` | `production` |
|---|---|---|---|
| 500+ error `detail` | Actual error message | Actual error message | `"Internal server error"` |
| `debug_detail` field | Included (full error chain) | Omitted | Omitted |
| Stack traces | Never (see note) | Never | Never |
| SQL / file paths | Never | Never | Never |
| `trace_id` | Always | Always | Always |
| 4xx error `detail` | Full detail | Full detail | Full detail (safe details only) |
| `Retry-After` header | Included | Included | Included |

> **Note:** Stack traces and raw SQL are never included in any environment — not even development. RFC 9457 Section 5 explicitly warns against exposing implementation internals. Use `trace_id` to correlate with server-side structured logs instead.

### Production Example (500)

```json
{
    "type": "/errors/internal",
    "title": "INTERNAL",
    "status": 500,
    "detail": "Internal server error",
    "trace_id": "abc-123-def-456"
}
```

### Development Example (500)

```json
{
    "type": "/errors/internal",
    "title": "INTERNAL",
    "status": 500,
    "detail": "connection refused: database not reachable at 127.0.0.1:5432",
    "trace_id": "abc-123-def-456",
    "debug_detail": "pool timed out waiting for a connection: connection refused (os error 111) at 127.0.0.1:5432\n\nCaused by:\n    connection refused (os error 111)"
}
```

### How It Works

The `is_development_env()` function in the `IntoResponse` impl reads from the `AppState` config. There is no compile-time `cfg` flag — the environment is runtime-configurable so that staging/QA environments on NAS hardware can get helpful detail without recompilation.

The `server_config.security` JSONB group includes:

```json
{
    "environment": "production",
    "force_secure_cookies": false,
    "csrf_protection": true,
    ...
}
```

Valid values: `development`, `staging`, `production`. Default: `production`.

## Implementation Rules

1. **Domain error enums** live in each module's `error.rs` file — close to the code that produces them
2. **`AppError`** is the only error type that crosses the HTTP boundary — all Axum handlers return `Result<impl IntoResponse, AppError>`
3. **Internal errors** (anyhow) are wrapped in `AppError::Internal` — clients see a generic 500 with a `trace_id`; the full error chain is logged server-side via `tracing`
4. **Validation errors** (`VALID_001`) always include the `errors` array with per-field code and message
5. **`trace_id`** is included in every error response — generated per request, propagated through `tracing` spans, used for log correlation
6. **Never expose internals** — no stack traces, SQL queries, file paths, or raw error messages in API responses, regardless of environment. Development mode adds `debug_detail` but never stack traces. See [API_SECURITY.md](../security/API_SECURITY.md) for the full error response sanitization policy
7. **Error codes are stable** — clients can depend on them across versions. Codes are appended, never renamed or removed
8. **Domain-to-HTTP mapping is centralized** — the `IntoResponse` impl for `AppError` is the single source of truth for which domain error maps to which HTTP status and error code
9. **Logging happens at the boundary** — when `AppError::Internal` is constructed, the full anyhow chain is logged at `ERROR` level with the trace_id. Domain-level errors that produce 4xx responses are logged at `WARN` or `DEBUG`
10. **Environment-aware detail** — 500+ errors hide internal messages in production; development/staging include `debug_detail` with the full error chain. Controlled by `server_config.security.environment`
11. **Retry-After on external failures** — 503 and 504 responses include a `Retry-After` header so clients know when to retry

## Implementation Notes

### Phase 3 — error.rs (Task 3)

`server/src/error.rs` implements the application-layer `AppError` enum and RFC 9457 response format. Implemented with generic variants only; domain-specific variants (`Auth(#[from] AuthError)`, `Database(#[from] DbError)`, etc.) will be added as each domain module is built in Phases 4–14.

**Implemented variants:** `NotFound`, `BadRequest`, `Conflict`, `Unauthorized`, `Forbidden`, `UnprocessableEntity`, `ServiceUnavailable`, `GatewayTimeout`, `Validation` (carries `Vec<FieldError>` + optional `instance` for `VALID_001`), `RateLimited` (carries error code string), `Internal` (wraps `anyhow::Error`).

**Deferred decisions:**
- `is_development_env()` reads from `OnceLock<String>` global set by `AppState::new()` (via `set_environment()`), falling back to `DUSKCUE_ENVIRONMENT` env var if not yet initialized. Wired in Phase 3, Task 2 (`state.rs`).
- `get_trace_id()` generates UUID v7 per error. Request-span propagation via `tracing` spans can be enhanced when middleware is implemented (Phase 3, Task 4).
- Domain `From` impls will be added as each domain's `error.rs` is created (Phase 4+).

## Research Sources

- Caroline Morton — Error Handling in Rust: anyhow and thiserror (January 2026)
- OneUptime — How to Design Error Types with thiserror and anyhow in Rust (January 2026)
- DEV Community — Error Handling in Axum (May 2026)
- Rustify — Rust for Backend Development: Complete Axum Guide 2026 (April 2026)
- Leapcell — Rust Error Handling Compared: anyhow vs thiserror vs snafu (April 2025)
- Reddit r/rust — Best Error Handling Crate discussion (November 2025)
- RFC 9457 — Problem Details for HTTP APIs (obsoletes RFC 7807): https://www.rfc-editor.org/rfc/rfc9457
- Youngju Kim — REST API Design Best Practices 2025: Error Response Design (March 2026)
- Levo.ai — REST API Security Best Practices 2026: verbose error risks, minimal exposure principle (November 2025)
- Stack Overflow — HTTP Status Code for External Dependency Error: 503 vs 502 vs 504 consensus
- MDN — 503 Service Unavailable: Retry-After header guidance (July 2025)
- Classifarr (github.com/cloudbyday90/Classifarr) — Express error handling patterns: AppError hierarchy, isOperational flag, environment-aware responses
