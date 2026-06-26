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

use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;
use validator::Validate;

use crate::error::{AppError, FieldError};
use crate::extractors::Require;
use crate::extractors::{CanManageScheduledTasks, CanManageServer};
use crate::state::AppState;

use super::service;
use super::types::{
    ConfigGroupResponse, ScheduledTaskCancelResponse, ScheduledTaskListResponse,
    ScheduledTaskResponse, ScheduledTaskRunListResponse, ScheduledTaskRunsQuery,
    ScheduledTaskTriggerResponse, ServerConfigResponse, UpdateConfigGroupRequest,
    UpdateServerConfigRequest, ValidateProviderRequest, ValidateProviderResponse,
};

fn validation_error(e: validator::ValidationErrors, instance: impl Into<String>) -> AppError {
    let errors: Vec<FieldError> = e
        .field_errors()
        .into_iter()
        .flat_map(|(field, errs)| {
            errs.iter().map(move |err| FieldError {
                field: field.to_string(),
                code: err.code.to_string(),
                message: err
                    .message
                    .as_ref()
                    .map(|m| m.to_string())
                    .unwrap_or_default(),
            })
        })
        .collect();
    AppError::Validation {
        errors,
        instance: Some(instance.into()),
    }
}

pub async fn get_server_config(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
) -> Result<Json<ServerConfigResponse>, AppError> {
    let result = service::get_server_config(&state).await?;
    Ok(Json(result))
}

pub async fn update_server_config(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
    Json(req): Json<UpdateServerConfigRequest>,
) -> Result<Json<ServerConfigResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/server/config"))?;

    let result = service::update_server_config(&state, &req.values).await?;
    Ok(Json(result))
}

pub async fn get_config_group(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
    Path(group): Path<String>,
) -> Result<Json<ConfigGroupResponse>, AppError> {
    let result = service::get_config_group(&state, &group).await?;
    Ok(Json(result))
}

pub async fn list_scheduled_tasks(
    _auth: Require<CanManageScheduledTasks>,
    State(state): State<AppState>,
) -> Result<Json<ScheduledTaskListResponse>, AppError> {
    let result = service::list_scheduled_tasks(&state).await?;
    Ok(Json(result))
}

pub async fn get_scheduled_task(
    _auth: Require<CanManageScheduledTasks>,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ScheduledTaskResponse>, AppError> {
    let result = service::get_scheduled_task(&state, task_id).await?;
    Ok(Json(result))
}

pub async fn trigger_scheduled_task(
    _auth: Require<CanManageScheduledTasks>,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ScheduledTaskTriggerResponse>, AppError> {
    let result = service::trigger_scheduled_task(&state, task_id).await?;
    Ok(Json(result))
}

pub async fn cancel_scheduled_task(
    _auth: Require<CanManageScheduledTasks>,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
) -> Result<Json<ScheduledTaskCancelResponse>, AppError> {
    let result = service::cancel_scheduled_task(&state, task_id).await?;
    Ok(Json(result))
}

pub async fn list_scheduled_task_runs(
    _auth: Require<CanManageScheduledTasks>,
    State(state): State<AppState>,
    Path(task_id): Path<Uuid>,
    Query(query): Query<ScheduledTaskRunsQuery>,
) -> Result<Json<ScheduledTaskRunListResponse>, AppError> {
    let result = service::list_scheduled_task_runs(
        &state,
        task_id,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(25),
    )
    .await?;
    Ok(Json(result))
}

pub async fn update_config_group(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
    Path(group): Path<String>,
    Json(req): Json<UpdateConfigGroupRequest>,
) -> Result<Json<ConfigGroupResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, format!("/api/v1/server/config/{group}")))?;

    let result = service::update_config_group(&state, &group, req.value).await?;
    Ok(Json(result))
}

pub async fn validate_provider_key(
    _auth: Require<CanManageServer>,
    State(_state): State<AppState>,
    Json(req): Json<ValidateProviderRequest>,
) -> Result<Json<ValidateProviderResponse>, AppError> {
    req.validate()
        .map_err(|e| validation_error(e, "/api/v1/settings/providers/validate"))?;

    req.validate_credentials()?;

    let result = service::validate_provider(
        &req.provider,
        req.access_token.as_deref(),
        req.api_key.as_deref(),
    )
    .await?;

    Ok(Json(result))
}
