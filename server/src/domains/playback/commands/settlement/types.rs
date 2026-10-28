// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub(super) struct IntentRow {
    pub household_id: Uuid,
    pub session_id: Uuid,
    pub credential_version: i64,
    pub transport: i16,
    pub profile_id: Option<Uuid>,
    pub profile_revision: Option<i64>,
    pub intent_hash: Option<Vec<u8>>,
    pub instance_id: Uuid,
    pub admission_fenced: bool,
    pub preparation_complete: bool,
    pub promoted: bool,
    pub observed_at: DateTime<Utc>,
    pub settled_at: Option<DateTime<Utc>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct WithdrawRequest {
    pub protocol: String,
}

#[derive(Serialize)]
pub struct SettlementResponse {
    pub protocol: &'static str,
    pub request_id: Uuid,
    pub status: &'static str,
    pub status_url: String,
    pub observed_at: DateTime<Utc>,
    pub admission_fenced: bool,
    pub preparation_complete: bool,
    pub parent_released: Option<bool>,
}
