// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use axum::extract::{Path, State};
use axum::Json;

use crate::error::AppError;
use crate::domains::quality::types::*;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

pub async fn report_capabilities(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<ReportCapabilitiesRequest>,
) -> Result<Json<DeviceProfileResponse>, AppError> {
    todo!()
}

pub async fn get_capabilities(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<DeviceProfileResponse>, AppError> {
    todo!()
}

pub async fn list_capability_tests(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<CapabilityTestListResponse>, AppError> {
    todo!()
}

pub async fn start_wizard(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<StartWizardRequest>,
) -> Result<Json<WizardStartResponse>, AppError> {
    todo!()
}

pub async fn submit_wizard_result(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_test_id): Path<uuid::Uuid>,
    Json(_req): Json<WizardTestResultRequest>,
) -> Result<Json<CapabilityTestResponse>, AppError> {
    todo!()
}

pub async fn get_bandwidth_probe(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<&'static [u8], AppError> {
    todo!()
}

pub async fn submit_bandwidth_probe_result(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<BandwidthProbeResultRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn submit_telemetry(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<SegmentTelemetryRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn submit_qoe(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<QoeReportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn admin_network_summary(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<NetworkQualitySummary>>, AppError> {
    todo!()
}

pub async fn admin_device_summary(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<DeviceCapabilitySummary>>, AppError> {
    todo!()
}

pub async fn admin_qoe_summary(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<QoeSummary>>, AppError> {
    todo!()
}

pub async fn admin_transcode_breakdown(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TranscodeBreakdown>, AppError> {
    todo!()
}
