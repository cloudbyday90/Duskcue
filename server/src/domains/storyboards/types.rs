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
use serde::Serialize;
use uuid::Uuid;

pub static VALID_STORYBOARD_WIDTHS: &[u32] = &[160, 320, 640];
pub static VALID_INTERVAL_MODES: &[&str] = &["adaptive", "fixed"];

pub struct StoryboardRow {
    pub id: Uuid,
    pub media_file_id: Uuid,
    pub file_hash: String,
    pub interval_seconds: i32,
    pub width: i32,
    pub height: i32,
    pub sprite_count: i32,
    pub total_thumbnails: i32,
    pub total_size_bytes: i64,
    pub keyframe_only: bool,
    pub quality: i32,
    pub generated_at: DateTime<Utc>,
    pub generation_duration_ms: Option<i32>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct SpriteResponse {
    pub url: String,
    pub thumbnails: i32,
    pub columns: i32,
    pub rows: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoryboardResponse {
    pub media_file_id: Uuid,
    pub interval_seconds: i32,
    pub width: i32,
    pub height: i32,
    pub sprite_count: i32,
    pub total_thumbnails: i32,
    pub index_url: String,
    pub sprites: Vec<SpriteResponse>,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateStoryboardsResponse {
    pub queued: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteStoryboardResponse {
    pub deleted: bool,
    pub media_item_id: Uuid,
}
