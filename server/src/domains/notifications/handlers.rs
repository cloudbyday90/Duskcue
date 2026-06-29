// Duskcue — Self-hosted media streaming server
// Copyright (C) 2026-2026 Duskcue Contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even implied
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use axum::Json;
use axum::extract::{Path, Query, State};
use uuid::Uuid;
use validator::Validate;

use crate::domains::notifications::service;
use crate::domains::notifications::types::*;
use crate::error::AppError;
use crate::extractors::{AuthenticatedUser, CanManageServer, Require};
use crate::services::notification_dispatch::{NotificationInput, dispatch};
use crate::state::AppState;

fn map_validation_errors(e: validator::ValidationErrors) -> AppError {
    use crate::error::FieldError;
    let errors: Vec<FieldError> = e
        .field_errors()
        .into_iter()
        .flat_map(|(field, errs)| {
            errs.iter().map(move |err| FieldError {
                field: field.to_string(),
                code: err.code.to_string(),
                message: err
                    .message
                    .clone()
                    .map(|m| m.to_string())
                    .unwrap_or_default(),
            })
        })
        .collect();
    AppError::Validation {
        errors,
        instance: None,
    }
}

pub async fn list_notifications(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Query(query): Query<NotificationListQuery>,
) -> Result<Json<NotificationListResponse>, AppError> {
    let result = service::list_notifications(&state.pool, user.user_id, &query).await?;
    Ok(Json(result))
}

pub async fn get_unread_count(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<UnreadCountResponse>, AppError> {
    let result = service::count_unread(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn mark_notification_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<MarkReadResponse>, AppError> {
    let result =
        service::mark_read(&state.pool, user.user_id, notification_id).await?;
    Ok(Json(result))
}

pub async fn mark_all_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<BulkMarkReadResponse>, AppError> {
    let result = service::mark_all_read(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn delete_notification(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(notification_id): Path<Uuid>,
) -> Result<Json<DeleteResponse>, AppError> {
    service::delete_notification(&state.pool, user.user_id, notification_id).await?;
    Ok(Json(DeleteResponse { deleted: true }))
}

pub async fn delete_read(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<BulkDeleteResponse>, AppError> {
    let result = service::delete_read(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn list_notification_types(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<NotificationTypeListResponse>, AppError> {
    let result = service::list_notification_types(&state.pool).await?;
    Ok(Json(result))
}

pub async fn list_user_preferences(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<NotificationPreferenceListResponse>, AppError> {
    let result = service::list_preferences(&state.pool, user.user_id).await?;
    Ok(Json(result))
}

pub async fn update_user_preference(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(type_id): Path<Uuid>,
    Json(req): Json<UpdatePreferenceRequest>,
) -> Result<Json<PreferenceUpdateResponse>, AppError> {
    req.validate().map_err(map_validation_errors)?;
    let result =
        service::update_preference(&state.pool, user.user_id, type_id, &req).await?;
    Ok(Json(result))
}

pub async fn send_test_notification(
    State(state): State<AppState>,
    auth: Require<CanManageServer>,
    Json(req): Json<TestNotificationRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    req.validate().map_err(map_validation_errors)?;

    let notification_type = req
        .notification_type
        .as_deref()
        .unwrap_or("server_alert");
    let mut input = NotificationInput::new(
        auth.user.user_id,
        notification_type,
        serde_json::json!({"message": "Test notification from admin panel"}),
    );
    input.title = req.title;
    input.body = req.body;

    let result = dispatch(&state, &input)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Notification dispatch failed: {}", e)))?;

    Ok(Json(serde_json::json!({
        "notification_id": result.notification_id,
        "in_app": result.in_app,
        "sse": result.sse,
        "webhook": result.webhook,
        "push": result.push,
    })))
}
