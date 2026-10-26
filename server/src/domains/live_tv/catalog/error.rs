// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::http::StatusCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("Live TV catalog request is invalid")]
    Invalid,
    #[error("Live TV catalog access is denied")]
    Forbidden,
    #[error("Live TV catalog entry is unavailable")]
    NotFound,
    #[error("Live TV catalog context changed; begin a new read")]
    Changed,
    #[error("Live TV catalog is unavailable")]
    Unavailable,
    #[error("Session expired")]
    Session,
}

impl CatalogError {
    pub fn http(self) -> (StatusCode, &'static str, String) {
        let (status, code) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "IPTV_CATALOG_001"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "IPTV_CATALOG_002"),
            Self::NotFound => (StatusCode::NOT_FOUND, "IPTV_CATALOG_003"),
            Self::Changed => (StatusCode::CONFLICT, "IPTV_CATALOG_004"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "IPTV_CATALOG_005"),
            Self::Session => (StatusCode::UNAUTHORIZED, "AUTH_005"),
        };
        (status, code, self.to_string())
    }
}

impl From<sqlx::Error> for CatalogError {
    fn from(_: sqlx::Error) -> Self {
        Self::Unavailable
    }
}

impl From<super::super::feed::imports::ImportError> for CatalogError {
    fn from(_: super::super::feed::imports::ImportError) -> Self {
        Self::Unavailable
    }
}
