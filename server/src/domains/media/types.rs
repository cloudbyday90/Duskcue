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

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

pub static VALID_MEDIA_ITEM_TYPES: &[&str] = &["movie", "series", "season", "episode"];
pub static VALID_MATCH_STATES: &[&str] = &["unmatched", "auto_matched", "confirmed", "manual"];
pub static VALID_IDENTIFICATION_SOURCES: &[&str] =
    &["media_match", "nfo", "provider_id_tag", "filename_parse", "manual"];

pub struct MediaItemRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub library_id: Uuid,
    pub r#type: String,
    pub title: String,
    pub sort_title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub content_rating: Option<String>,
    pub runtime_seconds: Option<i32>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub trakt_id: Option<i64>,
    pub rating_average: Option<f32>,
    pub rating_vote_count: Option<i32>,
    pub metadata: serde_json::Value,
    pub match_state: String,
    pub identification_source: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItemResponse {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub library_id: Uuid,
    pub r#type: String,
    pub title: String,
    pub sort_title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub content_rating: Option<String>,
    pub runtime_seconds: Option<i32>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub trakt_id: Option<i64>,
    pub rating_average: Option<f32>,
    pub rating_vote_count: Option<i32>,
    pub metadata: serde_json::Value,
    pub match_state: String,
    pub identification_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub episode_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub absolute_episode_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaItemListResponse {
    pub items: Vec<MediaItemResponse>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateMediaItemRequest {
    #[validate(length(min = 1, max = 500))]
    pub title: Option<String>,

    #[validate(length(max = 500))]
    pub sort_title: Option<String>,

    #[validate(length(max = 500))]
    pub original_title: Option<String>,

    pub overview: Option<String>,

    pub premiere_date: Option<NaiveDate>,

    pub end_date: Option<NaiveDate>,

    pub content_rating: Option<String>,

    pub runtime_seconds: Option<i32>,

    pub tmdb_id: Option<i64>,

    pub imdb_id: Option<String>,

    pub tvdb_id: Option<i64>,

    pub trakt_id: Option<i64>,

    pub rating_average: Option<f32>,

    pub rating_vote_count: Option<i32>,

    pub metadata: Option<serde_json::Value>,

    pub match_state: Option<String>,

    pub identification_source: Option<String>,
}

pub struct MediaFileRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub media_item_id: Uuid,
    pub file_path: String,
    pub file_size: i64,
    pub file_hash: Option<String>,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub container_format: String,
    pub video_codec: Option<String>,
    pub video_resolution: Option<String>,
    pub video_bitrate: Option<i32>,
    pub video_dynamic_range: Option<String>,
    pub video_frame_rate: Option<f64>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_language: Option<String>,
    pub audio_bitrate: Option<i32>,
    pub runtime_seconds: i32,
    pub last_scanned_at: DateTime<Utc>,
    pub is_healthy: bool,
    pub additional_streams: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct MediaFileResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub file_path: String,
    pub file_size: i64,
    pub file_hash: Option<String>,
    pub file_modified_at: Option<DateTime<Utc>>,
    pub container_format: String,
    pub video_codec: Option<String>,
    pub video_resolution: Option<String>,
    pub video_bitrate: Option<i32>,
    pub video_dynamic_range: Option<String>,
    pub video_frame_rate: Option<f64>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_language: Option<String>,
    pub audio_bitrate: Option<i32>,
    pub runtime_seconds: i32,
    pub last_scanned_at: DateTime<Utc>,
    pub is_healthy: bool,
    pub additional_streams: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
