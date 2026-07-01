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
use serde::Deserialize;
use uuid::Uuid;
use validator::Validate;

use crate::domains::tv::service as tv_service;
use crate::domains::tv::types::TvSurfaceSectionType;
use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanManageUsers, Require};
use crate::state::AppState;

use super::service;
use super::types::*;

#[derive(Debug, Clone, Deserialize)]
pub struct ListUsersQuery {
    page: Option<u32>,
    page_size: Option<u32>,
    status: Option<String>,
    role: Option<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
    _auth: Require<CanManageUsers>,
    Query(query): Query<ListUsersQuery>,
) -> Result<Json<UserListResponse>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(25).clamp(1, 100);

    let response = service::list_users(
        &state.pool,
        page,
        page_size,
        query.status.as_deref(),
        query.role.as_deref(),
    )
    .await?;

    Ok(Json(response))
}

pub async fn get_user(
    State(state): State<AppState>,
    _auth: Require<CanManageUsers>,
    axum::extract::Path(target_user_id): axum::extract::Path<Uuid>,
) -> Result<Json<UserResponse>, AppError> {
    let response = service::get_user(&state.pool, target_user_id).await?;

    Ok(Json(response))
}

pub async fn get_user_preferences(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<UserPreferencesResponse>, AppError> {
    let response = service::get_user_preferences(&state.pool, user.user_id).await?;

    Ok(Json(response))
}

pub async fn update_user_preferences(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<UpdateUserPreferencesRequest>,
) -> Result<Json<UserPreferencesResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some("/api/v1/user/preferences".to_string()),
    })?;

    let response = service::update_user_preferences(&state.pool, user.user_id, req.locale).await?;

    Ok(Json(response))
}

pub async fn update_user(
    State(state): State<AppState>,
    auth: Require<CanManageUsers>,
    axum::extract::Path(target_user_id): axum::extract::Path<Uuid>,
    Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, AppError> {
    req.validate().map_err(|e| AppError::Validation {
        errors: e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err
                        .message
                        .as_ref()
                        .map(|m| m.to_string())
                        .unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some(format!("/api/v1/users/{}", target_user_id)),
    })?;

    if let Some(policy_id) = req.streaming_policy_id {
        let exists = service::validate_streaming_policy_exists(&state.pool, policy_id).await?;
        if !exists {
            return Err(AppError::BadRequest("Streaming policy not found".into()));
        }
    }

    let tv_access_changed = req.has_all_library_access.is_some() || req.status.is_some();
    let response = service::update_user(
        &state.pool,
        service::UpdateUserParams {
            user_id: target_user_id,
            admin_user_id: auth.user.user_id,
            display_name: req.display_name,
            email: req.email,
            avatar_url: req.avatar_url,
            role: req.role,
            status: req.status,
            has_all_library_access: req.has_all_library_access,
            streaming_policy_id: req.streaming_policy_id,
            max_streams: req.max_streams,
            max_transcode_streams: req.max_transcode_streams,
            bandwidth_limit_bps: req.bandwidth_limit_bps,
        },
    )
    .await?;

    if tv_access_changed {
        tv_service::publish_tv_surface_changed(
            &state.event_bus,
            target_user_id,
            "access_changed",
            all_tv_sections(),
            None,
            None,
            None,
            0,
        );
    }

    Ok(Json(response))
}

pub async fn delete_user(
    State(state): State<AppState>,
    auth: Require<CanManageUsers>,
    axum::extract::Path(target_user_id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::soft_delete_user(&state.pool, target_user_id, auth.user.user_id).await?;
    tv_service::publish_tv_surface_changed(
        &state.event_bus,
        target_user_id,
        "access_changed",
        all_tv_sections(),
        None,
        None,
        None,
        0,
    );

    Ok(Json(serde_json::json!({ "status": "deleted" })))
}

fn all_tv_sections() -> Vec<TvSurfaceSectionType> {
    vec![
        TvSurfaceSectionType::Continue,
        TvSurfaceSectionType::NextUp,
        TvSurfaceSectionType::NewEpisodes,
        TvSurfaceSectionType::Recommended,
    ]
}
