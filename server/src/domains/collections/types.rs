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

pub static VALID_COLLECTION_TYPES: &[&str] = &["static", "dynamic", "smart"];
pub static VALID_VISIBILITY: &[&str] = &["visible", "hidden", "featured"];
pub static VALID_SYNC_MODES: &[&str] = &["sync", "append"];
pub static VALID_TEMPLATE_TYPES: &[&str] = &["single", "multi"];

pub static VALID_BUILDER_TYPES: &[&str] = &[
    "genre",
    "country",
    "decade",
    "content_rating",
    "actor",
    "director",
    "studio",
    "network",
    "franchise",
    "original_language",
    "year",
    "resolution",
    "audio_codec",
    "streaming_service",
    "tmdb_popular",
    "tmdb_top_rated",
    "tmdb_trending",
    "tmdb_now_playing",
    "tmdb_upcoming",
    "tmdb_collection",
    "trakt_trending",
    "trakt_popular",
    "trakt_recommended",
    "trakt_user_lists",
    "imdb_top_250",
    "custom_url",
];

pub struct CollectionRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub library_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub collection_type: String,
    pub visibility: String,
    pub is_dynamic: bool,
    pub dynamic_config: Option<serde_json::Value>,
    pub is_smart: bool,
    pub smart_filter: Option<serde_json::Value>,
    pub poster_artwork_id: Option<Uuid>,
    pub backdrop_artwork_id: Option<Uuid>,
    pub sort_order: i32,
    pub sort_by: String,
    pub item_count: i32,
    pub total_duration_seconds: i32,
    pub sync_mode: String,
    pub schedule: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_sync_result: Option<serde_json::Value>,
    pub is_enabled: bool,
    pub is_system: bool,
    pub metadata: serde_json::Value,
}

pub struct CollectionItemRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub collection_id: Uuid,
    pub media_item_id: Uuid,
    pub position: i32,
    pub is_missing: bool,
    pub missing_reason: Option<String>,
}

pub struct CollectionTemplateRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub name: String,
    pub description: Option<String>,
    pub template_type: String,
    pub template_json: serde_json::Value,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub is_system: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListCollectionsQuery {
    pub library_id: Option<Uuid>,
    pub collection_type: Option<String>,
    pub visibility: Option<String>,
    pub enabled: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListCollectionItemsQuery {
    pub include_missing: Option<bool>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateCollectionRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub library_id: Option<Uuid>,
    #[validate(length(max = 5000))]
    pub description: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub collection_type: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub visibility: Option<String>,
    pub dynamic_config: Option<serde_json::Value>,
    pub smart_filter: Option<serde_json::Value>,
    pub poster_artwork_id: Option<Uuid>,
    pub backdrop_artwork_id: Option<Uuid>,
    pub sort_order: Option<i32>,
    #[validate(length(min = 1, max = 100))]
    pub sort_by: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub sync_mode: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub schedule: Option<String>,
    pub is_enabled: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct UpdateCollectionRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: Option<String>,
    pub library_id: Option<Uuid>,
    #[validate(length(max = 5000))]
    pub description: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub collection_type: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub visibility: Option<String>,
    pub dynamic_config: Option<serde_json::Value>,
    pub smart_filter: Option<serde_json::Value>,
    pub poster_artwork_id: Option<Uuid>,
    pub backdrop_artwork_id: Option<Uuid>,
    pub sort_order: Option<i32>,
    #[validate(length(min = 1, max = 100))]
    pub sort_by: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub sync_mode: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub schedule: Option<String>,
    pub is_enabled: Option<bool>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct AddCollectionItemsRequest {
    #[validate(length(min = 1, max = 500))]
    pub media_item_ids: Vec<Uuid>,
    pub starting_position: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct CollectionItemPosition {
    pub media_item_id: Uuid,
    pub position: i32,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ReorderCollectionItemsRequest {
    #[validate(length(min = 1, max = 1000))]
    pub items: Vec<CollectionItemPosition>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SyncCollectionsRequest {
    pub library_id: Option<Uuid>,
    pub include_external: Option<bool>,
    pub reprocess_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SyncCollectionRequest {
    pub include_external: Option<bool>,
    pub reprocess_all: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct ImportCollectionTemplateRequest {
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    #[validate(length(max = 5000))]
    pub description: Option<String>,
    #[validate(length(min = 1, max = 20))]
    pub template_type: String,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub template_json: serde_json::Value,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionResponse {
    pub id: Uuid,
    pub library_id: Option<Uuid>,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub collection_type: String,
    pub visibility: String,
    pub is_dynamic: bool,
    pub dynamic_config: Option<serde_json::Value>,
    pub is_smart: bool,
    pub smart_filter: Option<serde_json::Value>,
    pub poster_artwork_id: Option<Uuid>,
    pub backdrop_artwork_id: Option<Uuid>,
    pub sort_order: i32,
    pub sort_by: String,
    pub item_count: i32,
    pub total_duration_seconds: i32,
    pub sync_mode: String,
    pub schedule: String,
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_sync_result: Option<serde_json::Value>,
    pub is_enabled: bool,
    pub is_system: bool,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionListResponse {
    pub items: Vec<CollectionResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionItemResponse {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub media_item_id: Uuid,
    pub position: i32,
    pub is_missing: bool,
    pub missing_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionItemsResponse {
    pub items: Vec<CollectionItemResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncCollectionResponse {
    pub status: String,
    pub queued_collections: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub template_type: String,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub is_system: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionTemplateResponse {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub template_type: String,
    pub template_json: serde_json::Value,
    pub author: Option<String>,
    pub source_url: Option<String>,
    pub is_system: bool,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
