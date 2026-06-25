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

pub static VALID_SEGMENT_TYPES: &[&str] = &["intro", "credits", "recap", "preview", "outro"];
pub static VALID_SEGMENT_SOURCES: &[&str] = &[
    "chapter",
    "chromaprint",
    "blackframe",
    "silence",
    "manual",
    "combined",
];

pub struct SegmentRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub media_item_id: Uuid,
    pub segment_type: String,
    pub start_ms: i32,
    pub end_ms: i32,
    pub skip_to_ms: i32,
    pub confidence: f32,
    pub source: String,
    pub is_manual: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SegmentListQuery {
    pub r#type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateSegmentRequest {
    #[validate(required)]
    pub segment_type: Option<String>,

    #[validate(range(min = 0))]
    pub start_ms: i32,

    #[validate(range(min = 1))]
    pub end_ms: i32,

    pub skip_to_ms: Option<i32>,

    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateSegmentRequest {
    #[validate(range(min = 0))]
    pub start_ms: Option<i32>,

    #[validate(range(min = 1))]
    pub end_ms: Option<i32>,

    pub skip_to_ms: Option<i32>,

    pub confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub segment_type: String,
    pub start_ms: i32,
    pub end_ms: i32,
    pub skip_to_ms: i32,
    pub confidence: f32,
    pub source: String,
    pub is_manual: bool,
    pub can_edit: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentListResponse {
    pub segments: Vec<SegmentResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeSegmentsResponse {
    pub library_id: Uuid,
    pub queued: bool,
    pub message: String,
}
