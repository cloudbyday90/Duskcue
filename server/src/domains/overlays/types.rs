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

pub static VALID_OVERLAY_TYPES: &[&str] = &["image", "text", "backdrop"];
pub static VALID_APPLIES_TO: &[&str] = &["poster", "backdrop", "season_poster", "episode_thumb"];
pub static VALID_HORIZONTAL_ALIGN: &[&str] = &["left", "center", "right"];
pub static VALID_VERTICAL_ALIGN: &[&str] = &["top", "center", "bottom"];

pub struct OverlayDefinitionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub name: String,
    pub slug: String,
    pub library_id: Option<Uuid>,
    pub overlay_type: String,
    pub image_path: Option<String>,
    pub text_template: Option<String>,
    pub font_family: String,
    pub font_size: i32,
    pub font_color: String,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<i32>,
    pub back_color: Option<String>,
    pub back_width: Option<i32>,
    pub back_height: Option<i32>,
    pub back_radius: Option<i32>,
    pub back_padding: Option<i32>,
    pub horizontal_offset: i32,
    pub horizontal_align: String,
    pub vertical_offset: i32,
    pub vertical_align: String,
    pub scale_width: Option<i32>,
    pub scale_height: Option<i32>,
    pub group_name: Option<String>,
    pub weight: i32,
    pub queue_name: Option<String>,
    pub conditions: serde_json::Value,
    pub suppresses: Vec<String>,
    pub applies_to: String,
    pub is_enabled: bool,
    pub is_system: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateOverlayRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(length(min = 1, max = 30))]
    pub overlay_type: String,
    pub library_id: Option<Uuid>,
    pub image_path: Option<String>,
    pub text_template: Option<String>,
    #[validate(length(max = 100))]
    pub font_family: Option<String>,
    #[validate(range(min = 1, max = 500))]
    pub font_size: Option<i32>,
    #[validate(length(max = 20))]
    pub font_color: Option<String>,
    #[validate(length(max = 20))]
    pub stroke_color: Option<String>,
    #[validate(range(min = 0, max = 50))]
    pub stroke_width: Option<i32>,
    #[validate(length(max = 20))]
    pub back_color: Option<String>,
    #[validate(range(min = 1))]
    pub back_width: Option<i32>,
    #[validate(range(min = 1))]
    pub back_height: Option<i32>,
    #[validate(range(min = 0))]
    pub back_radius: Option<i32>,
    #[validate(range(min = 0))]
    pub back_padding: Option<i32>,
    #[validate(range(min = 0, max = 1500))]
    pub horizontal_offset: Option<i32>,
    #[validate(length(min = 1, max = 10))]
    pub horizontal_align: Option<String>,
    #[validate(range(min = 0, max = 1500))]
    pub vertical_offset: Option<i32>,
    #[validate(length(min = 1, max = 10))]
    pub vertical_align: Option<String>,
    #[validate(range(min = 1))]
    pub scale_width: Option<i32>,
    #[validate(range(min = 1))]
    pub scale_height: Option<i32>,
    #[validate(length(max = 100))]
    pub group_name: Option<String>,
    #[validate(range(min = 0))]
    pub weight: Option<i32>,
    #[validate(length(max = 100))]
    pub queue_name: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub suppresses: Option<Vec<String>>,
    #[validate(length(min = 1, max = 20))]
    pub applies_to: Option<String>,
    pub is_enabled: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateOverlayRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub library_id: Option<Uuid>,
    pub image_path: Option<String>,
    pub text_template: Option<String>,
    #[validate(length(max = 100))]
    pub font_family: Option<String>,
    #[validate(range(min = 1, max = 500))]
    pub font_size: Option<i32>,
    #[validate(length(max = 20))]
    pub font_color: Option<String>,
    #[validate(length(max = 20))]
    pub stroke_color: Option<String>,
    #[validate(range(min = 0, max = 50))]
    pub stroke_width: Option<i32>,
    #[validate(length(max = 20))]
    pub back_color: Option<String>,
    #[validate(range(min = 1))]
    pub back_width: Option<i32>,
    #[validate(range(min = 1))]
    pub back_height: Option<i32>,
    #[validate(range(min = 0))]
    pub back_radius: Option<i32>,
    #[validate(range(min = 0))]
    pub back_padding: Option<i32>,
    #[validate(range(min = 0, max = 1500))]
    pub horizontal_offset: Option<i32>,
    #[validate(length(min = 1, max = 10))]
    pub horizontal_align: Option<String>,
    #[validate(range(min = 0, max = 1500))]
    pub vertical_offset: Option<i32>,
    #[validate(length(min = 1, max = 10))]
    pub vertical_align: Option<String>,
    #[validate(range(min = 1))]
    pub scale_width: Option<i32>,
    #[validate(range(min = 1))]
    pub scale_height: Option<i32>,
    pub group_name: Option<String>,
    #[validate(range(min = 0))]
    pub weight: Option<i32>,
    pub queue_name: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub suppresses: Option<Vec<String>>,
    #[validate(length(min = 1, max = 20))]
    pub applies_to: Option<String>,
    pub is_enabled: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ApplyOverlaysRequest {
    pub library_id: Option<Uuid>,
    #[validate(range(min = 1, max = 8))]
    pub max_concurrent: Option<i32>,
    pub reapply_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PreviewOverlayRequest {
    #[validate(required)]
    pub media_item_id: Option<Uuid>,
    pub overlay_ids: Option<Vec<Uuid>>,
    pub artwork_type: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct OverlayTemplateImport {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub version: Option<i32>,
    pub library_id: Option<Uuid>,
    #[validate(length(min = 1))]
    pub overlays: Vec<TemplateOverlayEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct TemplateOverlayEntry {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(length(min = 1, max = 30))]
    pub overlay_type: String,
    pub text_template: Option<String>,
    pub image_path: Option<String>,
    pub horizontal_align: Option<String>,
    pub vertical_align: Option<String>,
    pub horizontal_offset: Option<i32>,
    pub vertical_offset: Option<i32>,
    pub font_family: Option<String>,
    pub font_size: Option<i32>,
    pub font_color: Option<String>,
    pub back_color: Option<String>,
    pub back_radius: Option<i32>,
    pub group_name: Option<String>,
    #[validate(range(min = 0))]
    pub weight: Option<i32>,
    pub queue_name: Option<String>,
    pub conditions: Option<serde_json::Value>,
    pub suppresses: Option<Vec<String>>,
    pub applies_to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayDefinitionResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub library_id: Option<Uuid>,
    pub overlay_type: String,
    pub image_path: Option<String>,
    pub text_template: Option<String>,
    pub font_family: String,
    pub font_size: i32,
    pub font_color: String,
    pub stroke_color: Option<String>,
    pub stroke_width: Option<i32>,
    pub back_color: Option<String>,
    pub back_width: Option<i32>,
    pub back_height: Option<i32>,
    pub back_radius: Option<i32>,
    pub back_padding: Option<i32>,
    pub horizontal_offset: i32,
    pub horizontal_align: String,
    pub vertical_offset: i32,
    pub vertical_align: String,
    pub scale_width: Option<i32>,
    pub scale_height: Option<i32>,
    pub group_name: Option<String>,
    pub weight: i32,
    pub queue_name: Option<String>,
    pub conditions: serde_json::Value,
    pub suppresses: Vec<String>,
    pub applies_to: String,
    pub is_enabled: bool,
    pub is_system: bool,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayListResponse {
    pub items: Vec<OverlayDefinitionResponse>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyOverlaysResponse {
    pub status: String,
    pub queued_items: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreviewOverlayResponse {
    pub media_item_id: Uuid,
    pub artwork_type: String,
    pub applied_overlay_ids: Vec<Uuid>,
    pub preview_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayTemplateResponse {
    pub imported_count: usize,
    pub overlay_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverlayTemplateSummary {
    pub name: String,
    pub version: i32,
    pub overlay_count: usize,
}
