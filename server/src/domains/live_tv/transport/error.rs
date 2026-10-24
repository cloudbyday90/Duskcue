// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("Live transport destination is not authorized")]
    Destination,
    #[error("Live transport connection failed")]
    Connection,
    #[error("Live transport response is unsupported")]
    Response,
    #[error("Live transport input ended")]
    Ended,
    #[error("Live transport input is invalid")]
    Input,
    #[error("Live transport deadline exceeded")]
    Deadline,
    #[error("Live transport was cancelled")]
    Cancelled,
    #[error("Live transport capacity is unavailable")]
    Capacity,
    #[error("Live transport worker failed")]
    Worker,
    #[error("Live transport cache is unavailable or exceeded its budget")]
    Cache,
    #[error("Live transport output is unavailable")]
    Unavailable,
    #[error("Live transport cleanup is not confirmed")]
    Cleanup,
}
