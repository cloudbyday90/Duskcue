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

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use validator::Validate;

use crate::error::AppError;
use crate::domains::quality::service;
use crate::domains::quality::types::*;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct DeviceQuery {
    pub device_identifier: Option<String>,
}

pub async fn report_capabilities(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<ReportCapabilitiesRequest>,
) -> Result<Json<DeviceProfileResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/device/capabilities".to_string()),
    })?;
    let profile = service::report_capabilities(
        &state.pool,
        user.user_id,
        &req.device_identifier,
        &req,
    ).await?;
    Ok(Json(profile))
}

pub async fn get_capabilities(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<DeviceProfileResponse>, AppError> {
    let device_id = query.device_identifier.unwrap_or_default();
    if device_id.is_empty() {
        return Err(AppError::BadRequest(
            "device_identifier query parameter is required".to_string(),
        ));
    }
    let profile = service::get_device_profile(&state.pool, &device_id).await?;
    Ok(Json(profile))
}

pub async fn list_capability_tests(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<CapabilityTestListResponse>, AppError> {
    let device_id = query.device_identifier.unwrap_or_default();
    if device_id.is_empty() {
        return Err(AppError::BadRequest(
            "device_identifier query parameter is required".to_string(),
        ));
    }
    let tests = service::list_capability_tests(&state.pool, &device_id).await?;
    Ok(Json(tests))
}

pub async fn start_wizard(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<StartWizardRequest>,
) -> Result<Json<WizardStartResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/device/capability-tests/start".to_string()),
    })?;
    let result = service::start_wizard(
        &state.pool,
        user.user_id,
        &req.device_identifier,
    ).await?;
    Ok(Json(result))
}

pub async fn submit_wizard_result(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(test_id): Path<uuid::Uuid>,
    Json(req): Json<WizardTestResultRequest>,
) -> Result<Json<CapabilityTestResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some(format!("/api/v1/device/capability-tests/{}/result", test_id)),
    })?;
    let result = service::submit_wizard_test_result(
        &state.pool,
        test_id,
        &req,
    ).await?;
    Ok(Json(result))
}

pub async fn get_bandwidth_probe(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<&'static [u8], AppError> {
    static PROBE_PAYLOAD: [u8; 102400] = [0u8; 102400];
    Ok(&PROBE_PAYLOAD)
}

pub async fn submit_bandwidth_probe_result(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<BandwidthProbeResultRequest>,
) -> Result<Json<ProbeAckResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/probe/bandwidth/result".to_string()),
    })?;
    let result = service::submit_bandwidth_probe_result(
        &state.pool,
        user.user_id,
        &req,
    ).await?;
    Ok(Json(result))
}

pub async fn submit_telemetry(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<SegmentTelemetryRequest>,
) -> Result<Json<TelemetryAckResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/playback/telemetry".to_string()),
    })?;
    let config = state.runtime_config.load();
    let window = config.quality.throughput_estimate_window;
    let result = service::submit_segment_telemetry(
        &state.pool,
        user.user_id,
        &req,
        window,
    ).await?;
    Ok(Json(result))
}

pub async fn submit_qoe(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<QoeReportRequest>,
) -> Result<Json<QoeAckResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/playback/qoe".to_string()),
    })?;
    let config = state.runtime_config.load();
    let interval = config.quality.qoe_report_interval_seconds;
    let result = service::submit_qoe_report(
        &state.pool,
        user.user_id,
        &req,
        interval,
    ).await?;
    Ok(Json(result))
}

pub async fn admin_network_summary(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<NetworkQualitySummary>>, AppError> {
    let summary = service::get_network_quality_summary(&state.pool).await?;
    Ok(Json(summary))
}

pub async fn admin_device_summary(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<DeviceCapabilitySummary>>, AppError> {
    let summary = service::get_device_capability_summary(&state.pool).await?;
    Ok(Json(summary))
}

pub async fn admin_qoe_summary(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<Vec<QoeSummary>>, AppError> {
    let summary = service::get_qoe_summary(&state.pool).await?;
    Ok(Json(summary))
}

pub async fn admin_transcode_breakdown(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<TranscodeBreakdown>, AppError> {
    let breakdown = service::get_transcode_breakdown(&state.pool).await?;
    Ok(Json(breakdown))
}
