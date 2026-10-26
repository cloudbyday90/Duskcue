// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};

pub(crate) enum Content {
    File { id: usize, offset: u64, length: u64 },
    Bytes(Bytes),
    Empty,
}

pub(crate) struct Prepared {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub content: Content,
}
