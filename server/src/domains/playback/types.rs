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

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

pub static VALID_STREAM_DECISIONS: &[&str] = &["direct_play", "direct_stream", "transcode"];
pub static VALID_PLAYBACK_STATES: &[&str] = &["playing", "paused", "buffering", "stopped"];
pub static VALID_PLAYLIST_VISIBILITIES: &[&str] = &["private", "shared", "public"];

pub struct PlaySessionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub media_item_id: Uuid,
    pub library_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub stopped_at: Option<DateTime<Utc>>,
    pub paused_seconds: i32,
    pub duration_seconds: i32,
    pub ip_address: Option<String>,
    pub location_type: Option<String>,
    pub client_name: String,
    pub client_product: Option<String>,
    pub client_platform: Option<String>,
    pub client_version: Option<String>,
    pub client_device: Option<String>,
    pub is_secure: bool,
    pub bandwidth_bps: Option<i64>,
    pub quality_profile: Option<String>,
    pub stream_decision: String,
    pub percent_complete: Option<f32>,
    pub plays_in_session: i32,
    pub metadata: serde_json::Value,
}

pub struct UserItemDataRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub media_item_id: Uuid,
    pub is_watched: bool,
    pub play_count: i32,
    pub last_played_at: Option<DateTime<Utc>>,
    pub resume_position_ms: i32,
    pub last_played_media_file_id: Option<Uuid>,
    pub is_favorite: bool,
    pub user_rating: Option<i32>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
}

pub struct BookmarkRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub media_item_id: Uuid,
    pub position_ms: i32,
    pub label: String,
    pub description: Option<String>,
}

pub struct PlaylistRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub user_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub is_smart: bool,
    pub smart_filter: Option<serde_json::Value>,
    pub item_count: i32,
    pub total_duration_seconds: i32,
    pub metadata: serde_json::Value,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct StartPlaybackRequest {
    #[validate(required)]
    pub media_item_id: Option<Uuid>,
    pub media_file_id: Option<Uuid>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub max_streaming_bitrate: Option<u64>,
    pub force_transcode: Option<bool>,
    pub device_profile: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct HeartbeatRequest {
    #[validate(required)]
    pub session_id: Option<Uuid>,
    pub position_ms: Option<i32>,
    pub state: Option<String>,
    pub is_paused: Option<bool>,
    pub is_buffering: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SeekRequest {
    #[validate(required)]
    pub session_id: Option<Uuid>,
    #[validate(required)]
    pub position_ms: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateBookmarkRequest {
    #[validate(length(min = 1, max = 200))]
    pub label: String,
    pub description: Option<String>,
    #[validate(range(min = 0))]
    pub position_ms: i32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateBookmarkRequest {
    #[validate(length(min = 1, max = 200))]
    pub label: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreatePlaylistRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub description: Option<String>,
    #[validate(length(min = 1))]
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdatePlaylistRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub description: Option<String>,
    #[validate(length(min = 1))]
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AddPlaylistItemRequest {
    #[validate(required)]
    pub media_item_id: Option<Uuid>,
    pub position: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackStartResponse {
    pub session_id: Uuid,
    pub stream_decision: String,
    pub stream_url: String,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub source_video_codec: Option<String>,
    pub source_audio_codec: Option<String>,
    pub target_video_codec: Option<String>,
    pub target_audio_codec: Option<String>,
    pub transcode_session_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackInfoResponse {
    pub session_id: Uuid,
    pub media_item_id: Uuid,
    pub stream_decision: String,
    pub position_ms: i32,
    pub duration_ms: Option<i32>,
    pub transcode_progress: Option<f32>,
    pub is_paused: bool,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HeartbeatResponse {
    pub session_id: Uuid,
    pub position_ms: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserItemDataResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub is_watched: bool,
    pub play_count: i32,
    pub last_played_at: Option<DateTime<Utc>>,
    pub resume_position_ms: i32,
    pub is_favorite: bool,
    pub user_rating: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookmarkResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub position_ms: i32,
    pub label: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BookmarkListResponse {
    pub items: Vec<BookmarkResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub visibility: String,
    pub is_smart: bool,
    pub item_count: i32,
    pub total_duration_seconds: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistListResponse {
    pub items: Vec<PlaylistResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistItemResponse {
    pub id: Uuid,
    pub playlist_id: Uuid,
    pub media_item_id: Uuid,
    pub position: i32,
    pub title: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistItemListResponse {
    pub items: Vec<PlaylistItemResponse>,
    pub total: i64,
}
