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

#[derive(Debug, Clone, Deserialize)]
pub struct ApiMigrationConnectionConfig {
    pub method: Option<String>,
    pub base_url: String,
    pub api_key: Option<String>,
    pub api_key_hash: Option<String>,
    pub api_key_prefix: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlexMigrationConnectionConfig {
    pub method: Option<String>,
    pub original_filename: Option<String>,
    pub uploaded_at: Option<DateTime<Utc>>,
    pub file_size_bytes: Option<u64>,
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
    pub platform_user_id: Option<Uuid>,
    pub skip: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct StartMigrationRequest {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct MigrationSourceCredentialRequest {
    #[validate(length(min = 8, max = 4096))]
    pub api_key: Option<String>,
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
pub struct MigrationDiscoveryResponse {
    pub migration_source_id: Uuid,
    pub status: String,
    pub users_discovered: usize,
    pub users_mapped: usize,
    pub items_extracted: usize,
    pub items_inserted: u64,
    pub items_updated: u64,
    pub source_users: Vec<MigrationSourceUserResponse>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationSourceUserResponse {
    pub source_user_id: String,
    pub source_user_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationUserMappingOptionsResponse {
    pub migration_source_id: Uuid,
    pub saved_mappings: Vec<MigrationSavedUserMappingResponse>,
    pub platform_users: Vec<MigrationPlatformUserOptionResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationSavedUserMappingResponse {
    pub source_user_id: String,
    pub source_user_name: String,
    pub platform_user_id: Option<Uuid>,
    pub status: String,
    pub is_skipped: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationPlatformUserOptionResponse {
    pub platform_user_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub status: String,
    pub invitation_display_name: Option<String>,
    pub invitation_email: Option<String>,
    pub label: String,
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
pub struct MigrationPreflightResponse {
    pub migration_source_id: Uuid,
    pub platform: String,
    pub status: String,
    pub is_ready: bool,
    pub blockers: Vec<PreflightFinding>,
    pub warnings: Vec<PreflightFinding>,
    pub checks: Vec<PreflightCheck>,
    pub library_readiness: LibraryReadiness,
    pub user_mapping_readiness: UserMappingReadiness,
    pub source_readiness: SourceReadiness,
    pub disk_readiness: DiskReadiness,
    pub estimated_counts: PreflightEstimatedCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightFinding {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightCheck {
    pub name: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryReadiness {
    pub active_libraries: i64,
    pub scanned_libraries: i64,
    pub importable_items: i64,
    pub items_with_provider_ids: i64,
    pub provider_id_coverage_percent: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserMappingReadiness {
    pub mappings_total: i64,
    pub valid_mappings: i64,
    pub invalid_mappings: i64,
    pub skipped_mappings: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceReadiness {
    pub platform: String,
    pub method: String,
    pub reachable: Option<bool>,
    pub credential_mode: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskReadiness {
    pub required_bytes: Option<u64>,
    pub available_bytes: Option<u64>,
    pub has_headroom: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PreflightEstimatedCounts {
    pub source_items_discovered: i64,
    pub source_items_with_watch_data: i64,
    pub estimated_matches: i64,
    pub estimated_match_rate_percent: f32,
    pub low_confidence_count: i64,
    pub unmatched_count: i64,
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
