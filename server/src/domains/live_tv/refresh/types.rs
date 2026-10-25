// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
// Licensed under AGPL-3.0. See LICENSE for details.

use super::error::RefreshError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize)]
pub struct ControlResponse {
    pub id: Uuid,
    pub revision: i64,
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlRequest {
    pub enabled: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub playlist_pin: u8,
    pub guide_pin: u8,
}
impl StartRequest {
    pub fn validate(&self) -> Result<(), RefreshError> {
        if self.playlist_pin > 7 || self.guide_pin > 7 {
            Err(RefreshError::Invalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusQuery {
    pub attempt_id: Option<Uuid>,
}

#[derive(Serialize)]
pub struct ResourceResponse {
    pub assignment_id: Uuid,
    pub revision: i64,
    pub source_generation: i64,
    pub assignment_enabled: bool,
    pub playlist_address_count: i16,
    pub guide_address_count: i16,
    pub control: ControlResponse,
    pub latest_attempt_id: Option<Uuid>,
    pub latest_attempt_sequence: Option<i64>,
}

#[derive(Serialize)]
pub struct AcceptedResponse {
    pub attempt_id: Uuid,
    pub attempt_sequence: i64,
    pub status_url: String,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub latest: Option<super::super::feed::imports::ImportStatus>,
    pub expected_attempt_id: Option<Uuid>,
    pub observation: &'static str,
    pub locally_owned: bool,
    pub service_available: bool,
}
