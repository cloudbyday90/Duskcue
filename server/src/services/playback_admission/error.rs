// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionError {
    #[error("playback authority unavailable")]
    Authority,
    #[error("invalid streaming policy")]
    Policy,
    #[error("viewer capacity unavailable")]
    Capacity,
    #[error("request identity conflicts")]
    Conflict,
    #[error("playback request is still in progress")]
    Pending,
    #[error("playback owner fenced or closed")]
    Closed,
    #[error("reservation expired or retired")]
    Retired,
    #[error("playback storage unavailable")]
    Storage,
    #[error("playback transaction outcome is indeterminate")]
    Indeterminate,
    #[error("owned playback cleanup failed")]
    Cleanup,
}

impl From<sqlx::Error> for AdmissionError {
    fn from(_: sqlx::Error) -> Self {
        Self::Storage
    }
}

impl From<crate::domains::live_tv::catalog::error::CatalogError> for AdmissionError {
    fn from(_: crate::domains::live_tv::catalog::error::CatalogError) -> Self {
        Self::Authority
    }
}
