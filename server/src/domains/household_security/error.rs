// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OwnerError {
    #[error("fresh household-owner proof is required")]
    Required,
    #[error("household-owner proof is invalid or expired")]
    InvalidProof,
    #[error("household-owner proof is temporarily unavailable")]
    Locked,
    #[error("the last trusted household-owner factor cannot be removed")]
    LastFactor,
    #[error("household-management request is invalid")]
    InvalidRequest,
    #[error("household-management storage is unavailable")]
    Database(#[from] sqlx::Error),
    #[error("household-management service is unavailable")]
    Unavailable,
}

impl OwnerError {
    pub fn http(&self) -> (StatusCode, &'static str, String) {
        let (status, code) = match self {
            Self::Required => (StatusCode::FORBIDDEN, "OWNER_001"),
            Self::InvalidProof => (StatusCode::UNAUTHORIZED, "OWNER_002"),
            Self::Locked => (StatusCode::TOO_MANY_REQUESTS, "OWNER_003"),
            Self::LastFactor => (StatusCode::CONFLICT, "OWNER_004"),
            Self::InvalidRequest => (StatusCode::BAD_REQUEST, "OWNER_005"),
            Self::Database(_) | Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "OWNER_006"),
        };
        (status, code, self.to_string())
    }
}
