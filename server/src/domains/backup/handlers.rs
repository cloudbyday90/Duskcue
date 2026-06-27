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
use axum::extract::{Query, State};

use crate::error::{AppError, FieldError};
use crate::extractors::{CanManageServer, Require};
use crate::state::AppState;

use super::service;
use super::types::{
    BackupRunListResponse, BackupRunsQuery, BackupStatusResponse, BackupTaskListResponse,
};

pub async fn get_backup_status(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
) -> Result<Json<BackupStatusResponse>, AppError> {
    Ok(Json(service::get_backup_status(&state).await?))
}

pub async fn list_backup_tasks(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
) -> Result<Json<BackupTaskListResponse>, AppError> {
    Ok(Json(service::list_backup_tasks(&state).await?))
}

pub async fn list_backup_runs(
    _auth: Require<CanManageServer>,
    State(state): State<AppState>,
    Query(query): Query<BackupRunsQuery>,
) -> Result<Json<BackupRunListResponse>, AppError> {
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(AppError::Validation {
            errors: vec![FieldError {
                field: "limit".to_string(),
                code: "range".to_string(),
                message: "limit must be between 1 and 100".to_string(),
            }],
            instance: Some("/api/v1/backups/runs".to_string()),
        });
    }

    Ok(Json(service::list_backup_runs(&state, limit).await?))
}
