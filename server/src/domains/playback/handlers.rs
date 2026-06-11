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

use crate::error::AppError;
use crate::domains::playback::types::*;
use crate::extractors::AuthenticatedUser;
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
