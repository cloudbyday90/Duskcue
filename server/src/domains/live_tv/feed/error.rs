// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FeedError {
    #[error("saved feed authority is unavailable or changed")]
    Authority,
    #[error("saved feed operation capacity is unavailable")]
    Capacity,
    #[error("saved feed operation was cancelled")]
    Cancelled,
    #[error("saved feed operation exceeded its deadline")]
    Deadline,
    #[error("saved feed connection or TLS verification failed")]
    Connection,
    #[error("saved feed response is invalid or exceeds its limits")]
    Response,
    #[error("saved feed operation cleanup failed")]
    Cleanup,
}
