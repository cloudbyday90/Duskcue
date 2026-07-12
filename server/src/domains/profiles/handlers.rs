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
use axum::extract::{Path, State};
use axum::http::StatusCode;
use validator::Validate;

use crate::error::{AppError, FieldError};
use crate::extractors::AuthenticatedUser;
use crate::state::AppState;

use super::service;
use super::types::*;

pub async fn list_profiles(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<ProfileListResponse>, AppError> {
    Ok(Json(
        service::list_profiles(&state.pool, user.user_id, user.profile_id).await?,
    ))
}

pub async fn create_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateProfileRequest>,
) -> Result<(StatusCode, Json<ProfileResponse>), AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    validate_request(&req, "/api/v1/profiles")?;
    let profile = service::create_profile(&state.pool, user.user_id, req).await?;
    Ok((StatusCode::CREATED, Json(profile)))
}

pub async fn update_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(profile_id): Path<uuid::Uuid>,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<Json<ProfileResponse>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    validate_request(&req, &format!("/api/v1/profiles/{profile_id}"))?;
    Ok(Json(
        service::update_profile(&state.pool, user.user_id, profile_id, req).await?,
    ))
}

pub async fn delete_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(profile_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    service::delete_profile(&state.pool, user.user_id, profile_id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn switch_profile(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(profile_id): Path<uuid::Uuid>,
) -> Result<Json<SwitchProfileResponse>, AppError> {
    let active_profile =
        service::switch_profile(&state.pool, user.user_id, user.session_id, profile_id).await?;
    Ok(Json(SwitchProfileResponse { active_profile }))
}

pub async fn list_ambient_channels(
    State(state): State<AppState>,
    user: AuthenticatedUser,
) -> Result<Json<AmbientChannelListResponse>, AppError> {
    let scope = service::load_profile_scope(
        &state.pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await?;
    Ok(Json(
        service::list_ambient_channels(&state.pool, &scope, false).await?,
    ))
}

pub async fn create_ambient_channel(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<CreateAmbientChannelRequest>,
) -> Result<(StatusCode, Json<AmbientChannelResponse>), AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    validate_request(&req, "/api/v1/ambient-channels")?;
    let channel = service::create_ambient_channel(&state.pool, user.user_id, req).await?;
    Ok((StatusCode::CREATED, Json(channel)))
}

pub async fn update_ambient_channel(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(channel_id): Path<uuid::Uuid>,
    Json(req): Json<UpdateAmbientChannelRequest>,
) -> Result<Json<AmbientChannelResponse>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    validate_request(&req, &format!("/api/v1/ambient-channels/{channel_id}"))?;
    Ok(Json(
        service::update_ambient_channel(&state.pool, user.user_id, channel_id, req).await?,
    ))
}

pub async fn delete_ambient_channel(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(channel_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    service::delete_ambient_channel(&state.pool, user.user_id, channel_id).await?;
    Ok(Json(serde_json::json!({ "deleted": true })))
}

pub async fn get_ambient_channel_items(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(channel_id): Path<uuid::Uuid>,
) -> Result<Json<AmbientChannelItemsResponse>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    Ok(Json(
        service::list_ambient_channel_items(&state.pool, user.user_id, channel_id).await?,
    ))
}

pub async fn replace_ambient_channel_items(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(channel_id): Path<uuid::Uuid>,
    Json(req): Json<ReplaceAmbientChannelItemsRequest>,
) -> Result<Json<AmbientChannelItemsResponse>, AppError> {
    assert_profile_management_allowed(&state, &user).await?;
    service::replace_channel_items(&state.pool, user.user_id, channel_id, req.media_item_ids)
        .await?;
    Ok(Json(
        service::list_ambient_channel_items(&state.pool, user.user_id, channel_id).await?,
    ))
}

pub async fn next_ambient_channel_item(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(channel_id): Path<uuid::Uuid>,
    Json(req): Json<AmbientChannelNextRequest>,
) -> Result<Json<AmbientChannelNextResponse>, AppError> {
    let scope = service::load_profile_scope(
        &state.pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await?;
    Ok(Json(
        service::next_ambient_channel_item(
            &state.pool,
            &scope,
            channel_id,
            req.after_media_item_id,
        )
        .await?,
    ))
}

fn validate_request<T: Validate>(request: &T, instance: &str) -> Result<(), AppError> {
    request.validate().map_err(|error| AppError::Validation {
        errors: error
            .field_errors()
            .into_iter()
            .flat_map(|(field, errors)| {
                errors.iter().map(move |error| FieldError {
                    field: field.to_string(),
                    code: error.code.to_string(),
                    message: error
                        .message
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_default(),
                })
            })
            .collect(),
        instance: Some(instance.to_string()),
    })
}

async fn assert_profile_management_allowed(
    state: &AppState,
    user: &AuthenticatedUser,
) -> Result<(), AppError> {
    let scope = service::load_profile_scope(
        &state.pool,
        user.user_id,
        user.profile_id,
        user.has_all_library_access,
    )
    .await?;
    if service::is_kids(&scope) {
        return Err(super::error::ProfilesError::FeatureDisabled.into());
    }
    Ok(())
}
