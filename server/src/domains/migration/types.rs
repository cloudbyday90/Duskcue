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

pub static VALID_MIGRATION_PLATFORMS: &[&str] = &["plex", "jellyfin", "emby"];
pub static VALID_MIGRATION_STATUSES: &[&str] = &[
    "pending",
    "discovering",
    "matching",
    "importing",
    "completed",
    "failed",
    "cancelled",
];

pub struct MigrationSourceRow {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub platform: String,
    pub name: String,
    pub connection_config: serde_json::Value,
    pub last_run_at: Option<DateTime<Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListMigrationSourcesQuery {
    pub platform: Option<String>,
    pub status: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateMigrationSourceRequest {
    #[validate(length(min = 1, max = 20))]
    pub platform: String,
    #[validate(length(min = 1, max = 200))]
    pub name: String,
    pub connection_config: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct SaveUserMappingsRequest {
    #[validate(length(min = 1, max = 500))]
    pub mappings: Vec<UserMappingRequest>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct UserMappingRequest {
    #[validate(length(min = 1, max = 200))]
    pub source_user_id: String,
    #[validate(length(min = 1, max = 200))]
    pub source_user_name: String,
    pub platform_user_id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct StartMigrationRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnmatchedReportQuery {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationSourceResponse {
    pub id: Uuid,
    pub created_at: DateTime<Utc>,
    pub platform: String,
    pub name: String,
    pub connection_config: serde_json::Value,
    pub last_run_at: Option<DateTime<Utc>>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationSourceListResponse {
    pub items: Vec<MigrationSourceResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationActionResponse {
    pub migration_source_id: Uuid,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationProgressResponse {
    pub migration_source_id: Uuid,
    pub status: String,
    pub percent_complete: f32,
    pub items_discovered: i32,
    pub items_matched: i32,
    pub items_unmatched: i32,
    pub items_imported: i32,
    pub items_skipped: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnmatchedItemResponse {
    pub id: Uuid,
    pub source_item_id: String,
    pub source_item_title: String,
    pub source_item_type: String,
    pub source_item_year: Option<i32>,
    pub source_provider_ids: serde_json::Value,
    pub match_method: Option<String>,
    pub status: String,
    pub error_detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnmatchedReportResponse {
    pub items: Vec<UnmatchedItemResponse>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
}

impl From<MigrationSourceRow> for MigrationSourceResponse {
    fn from(row: MigrationSourceRow) -> Self {
        Self {
            id: row.id,
            created_at: row.created_at,
            platform: row.platform,
            name: row.name,
            connection_config: row.connection_config,
            last_run_at: row.last_run_at,
            status: row.status,
        }
    }
}
