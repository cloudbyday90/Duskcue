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

use crate::domains::segments::service;
use crate::domains::segments::types::*;
use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanManageLibraries, Require};
use crate::state::AppState;

fn can_edit_segment(user: &AuthenticatedUser) -> bool {
    user.role == "owner"
        || user
            .capabilities
            .iter()
            .any(|c| c == "can_manage_libraries")
}

pub async fn list_segments(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(item_id): Path<Uuid>,
    Query(query): Query<SegmentListQuery>,
) -> Result<Json<SegmentListResponse>, AppError> {
    let can_edit = can_edit_segment(&user);
    let result =
        service::list_segments(&state.pool, item_id, query.r#type.as_deref(), can_edit).await?;
    Ok(Json(result))
}

pub async fn create_segment(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(item_id): Path<Uuid>,
    Json(req): Json<CreateSegmentRequest>,
) -> Result<Json<SegmentResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
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
            instance: Some("/api/v1/items/{item_id}/segments".to_string()),
        }
    })?;

    let result = service::create_segment(&state.pool, item_id, &req).await?;
    Ok(Json(result))
}

pub async fn update_segment(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path((item_id, segment_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateSegmentRequest>,
) -> Result<Json<SegmentResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
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
            instance: Some("/api/v1/items/{item_id}/segments/{segment_id}".to_string()),
        }
    })?;

    let result = service::update_segment(&state.pool, item_id, segment_id, &req).await?;
    Ok(Json(result))
}

pub async fn delete_segment(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path((item_id, segment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_segment(&state.pool, item_id, segment_id).await?;
    Ok(Json(
        serde_json::json!({ "deleted": true, "segment_id": segment_id }),
    ))
}

pub async fn analyze_library_segments(
    State(state): State<AppState>,
    _auth: Require<CanManageLibraries>,
    Path(library_id): Path<Uuid>,
) -> Result<Json<AnalyzeSegmentsResponse>, AppError> {
    let result = service::trigger_library_analysis(&state, library_id).await?;
    Ok(Json(result))
}
