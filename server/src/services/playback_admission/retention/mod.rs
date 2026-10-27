// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

pub(super) mod database;
pub(crate) mod queries;
use chrono::{DateTime, Utc};
pub(crate) const REGULAR_OPERATIONS: usize = 7;
pub(crate) const MAINTENANCE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
#[derive(Debug, Clone, Copy)]
pub struct MaintenanceReceipt {
    pub deleted: usize,
    pub cutoff: DateTime<Utc>,
}
