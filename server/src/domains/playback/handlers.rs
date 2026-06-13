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
use axum::http::HeaderMap;
use axum::Json;
use sqlx::Row;
use validator::Validate;

use crate::error::AppError;
use crate::domains::playback::error::PlaybackError;
use crate::domains::playback::types::*;
use crate::domains::playback::service;
use crate::extractors::{AuthenticatedUser, Require, CanManageServer};
use crate::state::AppState;

pub async fn start_playback(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<StartPlaybackRequest>,
) -> Result<Json<PlaybackStartResponse>, AppError> {
    todo!()
}

pub async fn heartbeat(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, AppError> {
    todo!()
}

pub async fn stop_playback(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn seek(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<SeekRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn get_playback_info(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_session_id): Path<uuid::Uuid>,
) -> Result<Json<PlaybackInfoResponse>, AppError> {
    todo!()
}

pub async fn get_watch_data(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_item_id): Path<uuid::Uuid>,
) -> Result<Json<UserItemDataResponse>, AppError> {
    todo!()
}

pub async fn list_bookmarks(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_item_id): Path<uuid::Uuid>,
) -> Result<Json<BookmarkListResponse>, AppError> {
    todo!()
}

pub async fn create_bookmark(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_item_id): Path<uuid::Uuid>,
    Json(_req): Json<CreateBookmarkRequest>,
) -> Result<Json<BookmarkResponse>, AppError> {
    todo!()
}

pub async fn delete_bookmark(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path((_item_id, _bookmark_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn list_playlists(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
) -> Result<Json<PlaylistListResponse>, AppError> {
    todo!()
}

pub async fn get_playlist(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_playlist_id): Path<uuid::Uuid>,
) -> Result<Json<PlaylistResponse>, AppError> {
    todo!()
}

pub async fn create_playlist(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Json(_req): Json<CreatePlaylistRequest>,
) -> Result<Json<PlaylistResponse>, AppError> {
    todo!()
}

pub async fn update_playlist(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_playlist_id): Path<uuid::Uuid>,
    Json(_req): Json<UpdatePlaylistRequest>,
) -> Result<Json<PlaylistResponse>, AppError> {
    todo!()
}

pub async fn delete_playlist(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_playlist_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn list_playlist_items(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_playlist_id): Path<uuid::Uuid>,
) -> Result<Json<PlaylistItemListResponse>, AppError> {
    todo!()
}

pub async fn add_playlist_item(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_playlist_id): Path<uuid::Uuid>,
    Json(_req): Json<AddPlaylistItemRequest>,
) -> Result<Json<PlaylistItemResponse>, AppError> {
    todo!()
}

pub async fn remove_playlist_item(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path((_playlist_id, _item_id)): Path<(uuid::Uuid, uuid::Uuid)>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn stream_file(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_media_file_id): Path<uuid::Uuid>,
    _headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn get_transcode_manifest(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path(_session_id): Path<uuid::Uuid>,
) -> Result<Json<String>, AppError> {
    todo!()
}

pub async fn get_transcode_playlist(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path((_session_id, _rendition)): Path<(uuid::Uuid, String)>,
) -> Result<Json<String>, AppError> {
    todo!()
}

pub async fn get_transcode_segment(
    State(_state): State<AppState>,
    _user: AuthenticatedUser,
    Path((_session_id, _rendition, _segment)): Path<(uuid::Uuid, String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    todo!()
}

pub async fn list_streaming_policies(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
) -> Result<Json<StreamingPolicyListResponse>, AppError> {
    let result = service::list_streaming_policies(&state.pool).await?;
    Ok(Json(result))
}

pub async fn get_streaming_policy(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
    Path(policy_id): Path<uuid::Uuid>,
) -> Result<Json<StreamingPolicyResponse>, AppError> {
    let result = service::get_streaming_policy(&state.pool, policy_id).await?;
    Ok(Json(result))
}

pub async fn create_streaming_policy(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
    Json(req): Json<CreateStreamingPolicyRequest>,
) -> Result<Json<StreamingPolicyResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.clone().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect();
        AppError::Validation { errors, instance: None }
    })?;
    let result = service::create_streaming_policy(&state.pool, &req).await?;
    Ok(Json(result))
}

pub async fn update_streaming_policy(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
    Path(policy_id): Path<uuid::Uuid>,
    Json(req): Json<UpdateStreamingPolicyRequest>,
) -> Result<Json<StreamingPolicyResponse>, AppError> {
    req.validate().map_err(|e| {
        let errors: Vec<crate::error::FieldError> = e
            .field_errors()
            .into_iter()
            .flat_map(|(field, errs)| {
                errs.iter().map(move |err| crate::error::FieldError {
                    field: field.to_string(),
                    code: err.code.to_string(),
                    message: err.message.clone().map(|m| m.to_string()).unwrap_or_default(),
                })
            })
            .collect();
        AppError::Validation { errors, instance: None }
    })?;
    let result = service::update_streaming_policy(&state.pool, policy_id, &req).await?;
    Ok(Json(result))
}

pub async fn delete_streaming_policy(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
    Path(policy_id): Path<uuid::Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    service::delete_streaming_policy(&state.pool, policy_id).await?;
    Ok(Json(serde_json::json!({"deleted": true})))
}

pub async fn get_effective_streaming_limits(
    State(state): State<AppState>,
    _auth: Require<CanManageServer>,
    Path(user_id): Path<uuid::Uuid>,
) -> Result<Json<ResolvedStreamingLimitsResponse>, AppError> {
    let row = sqlx::query(
        "SELECT streaming_policy_id, max_streams, max_transcode_streams, bandwidth_limit_bps \
         FROM users WHERE id = $1 AND deleted_at IS NULL"
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(PlaybackError::from)?;

    let row = row.ok_or(AppError::NotFound("User not found".into()))?;

    let streaming_policy_id: Option<uuid::Uuid> = row.try_get("streaming_policy_id").ok().flatten();
    let max_streams: Option<i32> = row.try_get("max_streams").ok().flatten();
    let max_transcode_streams: Option<i32> = row.try_get("max_transcode_streams").ok().flatten();
    let bandwidth_limit_bps: Option<i64> = row.try_get("bandwidth_limit_bps").ok().flatten();

    let result = service::resolve_streaming_limits(
        &state.pool,
        user_id,
        max_streams,
        max_transcode_streams,
        bandwidth_limit_bps,
        streaming_policy_id,
    )
    .await?;

    Ok(Json(result))
}
