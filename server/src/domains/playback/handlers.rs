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

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use axum::Json;
use sqlx::Row;
use validator::Validate;

use crate::error::AppError;
use crate::domains::playback::error::PlaybackError;
use crate::domains::playback::service::{self, RangeSpec};
use crate::domains::playback::types::*;
use crate::extractors::{AuthenticatedUser, Require, CanManageServer};
use crate::state::AppState;

pub async fn start_playback(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<StartPlaybackRequest>,
) -> Result<Json<PlaybackStartResponse>, AppError> {
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

    let config = state.runtime_config.load();
    let result = service::start_playback(
        &state.pool,
        &state.transcode_manager,
        user.user_id,
        &user.role,
        &req,
        &config,
        &state.bootstrap.data_dir,
    )
    .await?;
    drop(config);

    Ok(Json(result))
}

pub async fn heartbeat(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<HeartbeatRequest>,
) -> Result<Json<HeartbeatResponse>, AppError> {
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

    let session_id = req.session_id.ok_or_else(|| {
        let errors = vec![crate::error::FieldError {
            field: "session_id".to_string(),
            code: "required".to_string(),
            message: "session_id is required".to_string(),
        }];
        AppError::Validation { errors, instance: None }
    })?;

    let result = service::heartbeat(
        &state.pool,
        user.user_id,
        session_id,
        req.position_ms,
        req.state.as_deref(),
        req.is_paused,
        req.is_buffering,
    )
    .await?;

    Ok(Json(result))
}

pub async fn stop_playback(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<StopPlaybackRequest>,
) -> Result<Json<StopPlaybackResponse>, AppError> {
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

    let session_id = req.session_id.ok_or_else(|| {
        let errors = vec![crate::error::FieldError {
            field: "session_id".to_string(),
            code: "required".to_string(),
            message: "session_id is required".to_string(),
        }];
        AppError::Validation { errors, instance: None }
    })?;

    let result = service::stop_playback(
        &state.pool,
        &state.transcode_manager,
        user.user_id,
        session_id,
        req.position_ms,
    )
    .await?;

    Ok(Json(result))
}

pub async fn seek(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Json(req): Json<SeekRequest>,
) -> Result<Json<SeekResponse>, AppError> {
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

    let session_id = req.session_id.ok_or_else(|| {
        let errors = vec![crate::error::FieldError {
            field: "session_id".to_string(),
            code: "required".to_string(),
            message: "session_id is required".to_string(),
        }];
        AppError::Validation { errors, instance: None }
    })?;

    let position_ms = req.position_ms.ok_or_else(|| {
        let errors = vec![crate::error::FieldError {
            field: "position_ms".to_string(),
            code: "required".to_string(),
            message: "position_ms is required".to_string(),
        }];
        AppError::Validation { errors, instance: None }
    })?;

    let result = service::seek(
        &state.pool,
        &state.transcode_manager,
        user.user_id,
        session_id,
        position_ms,
        &state.bootstrap.data_dir,
    )
    .await?;

    Ok(Json(result))
}

pub async fn get_playback_info(
    State(state): State<AppState>,
    user: AuthenticatedUser,
    Path(session_id): Path<uuid::Uuid>,
) -> Result<Json<PlaybackInfoResponse>, AppError> {
    let result =
        service::get_playback_info(&state.pool, &state.transcode_manager, user.user_id, session_id)
            .await?;

    Ok(Json(result))
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
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(media_file_id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let file_path = service::get_media_file_path(&state.pool, media_file_id).await?;
    let file_size = service::get_media_file_size(&state.pool, media_file_id).await?;

    let range_header = headers.get("range").and_then(|v| v.to_str().ok());
    let range = RangeSpec::parse(range_header, file_size)?;

    let content_type = service::guess_content_type(&file_path);

    match range {
        Some(range) => {
            let length = range.content_length() as usize;
            let mut file = tokio::fs::File::open(&file_path)
                .await
                .map_err(|_| PlaybackError::FileNotFound)?;

            use tokio::io::{AsyncReadExt, AsyncSeekExt};
            file.seek(std::io::SeekFrom::Start(range.start))
                .await
                .map_err(|_| PlaybackError::FileNotFound)?;

            let mut buffer = vec![0u8; length];
            file.read_exact(&mut buffer)
                .await
                .map_err(|_| PlaybackError::FileNotFound)?;

            Ok(Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, length.to_string())
                .header(header::CONTENT_RANGE, range.content_range_header())
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Body::from(buffer))
                .unwrap())
        }
        None => {
            let data = tokio::fs::read(&file_path)
                .await
                .map_err(|_| PlaybackError::FileNotFound)?;

            Ok(Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
                .header(header::CONTENT_LENGTH, file_size.to_string())
                .header(header::ACCEPT_RANGES, "bytes")
                .body(Body::from(data))
                .unwrap())
        }
    }
}

pub async fn get_transcode_manifest(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path(session_id): Path<uuid::Uuid>,
) -> Result<Response, AppError> {
    let content = service::get_transcode_manifest(&state.transcode_manager, session_id).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::from(content))
        .unwrap())
}

pub async fn get_transcode_playlist(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((session_id, rendition)): Path<(uuid::Uuid, String)>,
) -> Result<Response, AppError> {
    let content =
        service::get_transcode_playlist(&state.transcode_manager, session_id, &rendition).await?;

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .body(Body::from(content))
        .unwrap())
}

pub async fn get_transcode_segment(
    State(state): State<AppState>,
    _user: AuthenticatedUser,
    Path((session_id, rendition, segment)): Path<(uuid::Uuid, String, String)>,
) -> Result<Response, AppError> {
    let data =
        service::get_transcode_segment(&state.transcode_manager, session_id, &rendition, &segment)
            .await?;

    let content_type = if segment.ends_with(".m4s") {
        "video/iso.segment"
    } else if segment.ends_with(".mp4") || segment.ends_with(".m4v") {
        "video/mp4"
    } else {
        "application/octet-stream"
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, data.len().to_string())
        .header(header::CACHE_CONTROL, "max-age=3600")
        .body(Body::from(data))
        .unwrap())
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
