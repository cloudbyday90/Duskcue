// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("import authority or attempt is no longer current")]
    Authority,
    #[error("import capacity is unavailable")]
    Capacity,
    #[error("import was cancelled")]
    Cancelled,
    #[error("import exceeded its deadline")]
    Deadline,
    #[error("import feed could not be fetched")]
    Network,
    #[error("import format is unsupported or malformed")]
    Format,
    #[error("import exceeded its resource limits")]
    Limit,
    #[error("import storage or authentication failed")]
    Storage,
    #[error("import commit outcome is indeterminate")]
    Indeterminate,
    #[error("import cleanup did not complete")]
    Cleanup,
}

impl ImportError {
    pub(super) fn category(self) -> &'static str {
        match self {
            Self::Authority => "authority",
            Self::Capacity => "capacity",
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
            Self::Network => "network",
            Self::Format => "format",
            Self::Limit => "limit",
            Self::Storage => "storage",
            Self::Indeterminate => "indeterminate",
            Self::Cleanup => "cleanup",
        }
    }
}

impl From<sqlx::Error> for ImportError {
    fn from(_: sqlx::Error) -> Self {
        Self::Storage
    }
}

impl From<super::super::FeedError> for ImportError {
    fn from(value: super::super::FeedError) -> Self {
        use super::super::FeedError;
        match value {
            FeedError::Authority => Self::Authority,
            FeedError::Cancelled => Self::Cancelled,
            FeedError::Deadline => Self::Deadline,
            FeedError::Capacity => Self::Capacity,
            FeedError::Cleanup => Self::Cleanup,
            _ => Self::Network,
        }
    }
}
