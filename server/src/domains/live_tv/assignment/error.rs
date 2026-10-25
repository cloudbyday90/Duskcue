// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssignmentError {
    #[error("Live TV management operation is not allowed")]
    Forbidden,
    #[error("Live TV assignment or profile is unavailable")]
    NotFound,
    #[error("Live TV configuration changed; read it again before updating")]
    Stale,
    #[error("a current version-specific conditional request is required")]
    ConditionRequired,
    #[error("Live TV management request is malformed")]
    InvalidRequest,
    #[error("Live TV policy is invalid")]
    InvalidPolicy,
    #[error("Live TV configuration storage is unavailable")]
    Database(#[from] sqlx::Error),
    #[error("Live TV configuration service is unavailable")]
    Unavailable,
}

impl AssignmentError {
    pub fn http(&self) -> (StatusCode, &'static str, String) {
        let (status, code) = match self {
            Self::Forbidden => (StatusCode::FORBIDDEN, "IPTV_001"),
            Self::NotFound => (StatusCode::NOT_FOUND, "IPTV_002"),
            Self::Stale => (StatusCode::PRECONDITION_FAILED, "IPTV_003"),
            Self::ConditionRequired => (StatusCode::PRECONDITION_REQUIRED, "IPTV_004"),
            Self::InvalidRequest => (StatusCode::BAD_REQUEST, "IPTV_005"),
            Self::InvalidPolicy => (StatusCode::UNPROCESSABLE_ENTITY, "IPTV_006"),
            Self::Database(_) | Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "IPTV_007"),
        };
        (status, code, self.to_string())
    }
}
