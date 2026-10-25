// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("manual refresh request is malformed")]
    Invalid,
    #[error("manual refresh operation is forbidden")]
    Forbidden,
    #[error("manual refresh assignment is unavailable")]
    NotFound,
    #[error("manual refresh service is disabled, busy or unavailable")]
    Unavailable,
    #[error("manual refresh acceptance is uncertain; read status without replaying")]
    Uncertain,
    #[error("manual refresh cleanup did not complete")]
    Cleanup,
}
impl RefreshError {
    pub fn http(&self) -> (StatusCode, &'static str, String) {
        let (s, c) = match self {
            Self::Invalid => (StatusCode::BAD_REQUEST, "IPTV_REFRESH_001"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "IPTV_REFRESH_002"),
            Self::NotFound => (StatusCode::NOT_FOUND, "IPTV_REFRESH_003"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "IPTV_REFRESH_004"),
            Self::Uncertain => (StatusCode::SERVICE_UNAVAILABLE, "IPTV_REFRESH_005"),
            Self::Cleanup => (StatusCode::SERVICE_UNAVAILABLE, "IPTV_REFRESH_006"),
        };
        (s, c, self.to_string())
    }
}
