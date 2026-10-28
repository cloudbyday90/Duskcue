// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

mod checkpoint;
mod database;
pub(super) mod handlers;
mod intent;
mod projection;
mod recovery;
mod service;
pub(super) mod types;

pub(super) use service::execute;

use super::output::Output;
use std::sync::Arc;
use uuid::Uuid;

pub(super) struct Work {
    operation: Uuid,
    previous: Arc<Output>,
    target: Arc<Output>,
}
