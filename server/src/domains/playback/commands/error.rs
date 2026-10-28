// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use crate::services::playback_admission::AdmissionError;
use axum::{
    Json,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

#[derive(Debug, Clone, Copy, thiserror::Error)]
pub enum CommandError {
    #[error("The playback command is invalid")]
    Invalid,
    #[error("The playback command context or intent changed")]
    Conflict,
    #[error("Playback permission or policy is unavailable")]
    Forbidden,
    #[error("The playback command reconciliation window expired")]
    Expired,
    #[error("Playback capacity is temporarily unavailable")]
    Capacity,
    #[error("Playback is temporarily unavailable; reconcile explicitly")]
    Unavailable,
    #[error("The current playback configuration condition is required")]
    ConditionRequired,
    #[error("The playback configuration condition changed")]
    ConditionFailed,
    #[error("The playback request body exceeds its limit")]
    TooLarge,
    #[error("The playback request requires application/json")]
    MediaType,
    #[error("A playback operation is already in progress")]
    Busy,
}
impl IntoResponse for CommandError {
    fn into_response(self) -> Response {
        let (status, code) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "VOD_COMMAND_001"),
            Self::Conflict => (StatusCode::CONFLICT, "VOD_COMMAND_002"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "VOD_COMMAND_003"),
            Self::Expired => (StatusCode::GONE, "VOD_COMMAND_004"),
            Self::Capacity => (StatusCode::SERVICE_UNAVAILABLE, "VOD_COMMAND_005"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "VOD_COMMAND_006"),
            Self::ConditionRequired => (StatusCode::PRECONDITION_REQUIRED, "VOD_COMMAND_007"),
            Self::ConditionFailed => (StatusCode::PRECONDITION_FAILED, "VOD_COMMAND_008"),
            Self::TooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "VOD_COMMAND_009"),
            Self::MediaType => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "VOD_COMMAND_010"),
            Self::Busy => (StatusCode::CONFLICT, "VOD_COMMAND_011"),
        };
        let mut response=(status,Json(serde_json::json!({"type":"about:blank","title":status.canonical_reason().unwrap_or("Playback command failure"),"status":status.as_u16(),"detail":self.to_string(),"code":code}))).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/problem+json"),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            header::HeaderValue::from_static("private, no-store"),
        );
        if status == StatusCode::SERVICE_UNAVAILABLE {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, header::HeaderValue::from_static("5"));
        }
        response
    }
}
impl From<AdmissionError> for CommandError {
    fn from(value: AdmissionError) -> Self {
        match value {
            AdmissionError::Authority | AdmissionError::Policy | AdmissionError::Retired => {
                Self::Forbidden
            }
            AdmissionError::Conflict => Self::Conflict,
            AdmissionError::Precondition => Self::ConditionFailed,
            AdmissionError::Expired => Self::Expired,
            AdmissionError::RequestTime => Self::Invalid,
            AdmissionError::Capacity | AdmissionError::ReceiptCapacity => Self::Capacity,
            _ => Self::Unavailable,
        }
    }
}
