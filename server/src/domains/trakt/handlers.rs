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

use axum::extract::{Query, State};
use axum::Json;
use validator::Validate;

use crate::error::AppError;
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

use super::service;
use super::types::*;

pub async fn get_account(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<TraktAccountResponse>, AppError> {
    let result = service::get_account(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn start_link(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<DeviceCodeResponse>, AppError> {
    let result = service::start_device_link(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn poll_link(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<PollDeviceCodeRequest>,
) -> Result<Json<TraktAccountResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect();
        AppError::Validation {
            errors,
            instance: Some("/api/v1/trakt/account/poll".to_string()),
        }
    })?;

    let result = service::poll_device_code(&state.pool, user.user_id, &req.device_code).await?;
    Ok(Json(result))
}

pub async fn unlink_account(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, AppError> {
    service::unlink_account(&state.pool, user.user_id).await?;
    Ok(Json(serde_json::json!({ "unlinked": true })))
}

pub async fn get_settings(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<SyncSettingsResponse>, AppError> {
    let result = service::get_sync_settings(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn update_settings(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<UpdateSyncSettingsRequest>,
) -> Result<Json<SyncSettingsResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.as_ref().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect();
        AppError::Validation {
            errors,
            instance: Some("/api/v1/trakt/settings".to_string()),
        }
    })?;

    let result = service::update_sync_settings(&state.pool, user.user_id, &req).await?;
    Ok(Json(result))
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<SyncTriggerResponse>, AppError> {
    let result = service::trigger_sync(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn get_sync_status(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<SyncStatusResponse>, AppError> {
    let result = service::get_sync_status(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn list_history(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<TraktHistoryResponse>, AppError> {
    let result = service::list_history(&state.pool, user.user_id, &query).await?;
    Ok(Json(result))
}

pub async fn list_ratings(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<HistoryQuery>,
) -> Result<Json<TraktHistoryResponse>, AppError> {
    let result = service::list_ratings(&state.pool, user.user_id, &query).await?;
    Ok(Json(result))
}
