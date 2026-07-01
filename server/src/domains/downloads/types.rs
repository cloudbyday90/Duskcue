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
use serde_json::Value;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Clone)]
pub struct DownloadJobRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_session_id: Option<Uuid>,
    pub device_identifier: String,
    pub library_id: Uuid,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub status: String,
    pub package_format: String,
    pub package_strategy: String,
    pub quality_mode: String,
    pub progress_percent: f32,
    pub bytes_expected: Option<i64>,
    pub bytes_prepared: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub metadata: Value,
}

#[derive(Debug, Clone)]
pub struct DownloadPackageRow {
    pub id: Uuid,
    pub download_job_id: Uuid,
    pub user_id: Uuid,
    pub device_identifier: String,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub status: String,
    pub package_format: String,
    pub manifest_version: i32,
    pub manifest_relative_path: String,
    pub storage_key: String,
    pub total_bytes: i64,
    pub file_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct DownloadPackageFileRow {
    pub id: Uuid,
    pub download_package_id: Uuid,
    pub relative_path: String,
    pub file_role: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub checksum_sha256: String,
    pub segment_index: Option<i32>,
    pub track_type: Option<String>,
    pub track_identifier: Option<String>,
    pub is_required: bool,
}

#[derive(Debug, Clone)]
pub struct DownloadDeviceStateRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_session_id: Option<Uuid>,
    pub download_package_id: Uuid,
    pub device_identifier: String,
    pub local_status: String,
    pub bytes_downloaded: i64,
    pub files_verified: i32,
    pub last_online_check_at: Option<DateTime<Utc>>,
    pub last_played_at: Option<DateTime<Utc>>,
    pub local_resume_position_ms: i64,
    pub pending_sync: Value,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadClientPlatform {
    Android,
    Ios,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadQualityMode {
    Auto,
    DataSaver,
    Standard,
    Maximum,
    Manual,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPackageFormat {
    HlsFmp4,
    Mp4,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadJobStatus {
    Queued,
    Preparing,
    Ready,
    Failed,
    Cancelled,
    Expired,
    Revoked,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadLocalStatus {
    NotDownloaded,
    Downloading,
    Paused,
    Playable,
    Failed,
    Expired,
    Revoked,
    Deleted,
    SyncPending,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadPlanQuery {
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: Option<String>,
    pub client_platform: Option<DownloadClientPlatform>,
    pub quality_mode: Option<DownloadQualityMode>,
    pub media_file_id: Option<Uuid>,
    pub include_storyboards: Option<bool>,
    pub include_artwork: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateDownloadJobRequest {
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: String,
    #[validate(length(max = 128))]
    pub device_name: Option<String>,
    pub client_platform: DownloadClientPlatform,
    #[validate(length(max = 64))]
    pub client_version: Option<String>,
    pub quality_mode: DownloadQualityMode,
    #[serde(default)]
    pub selected_audio: Value,
    #[serde(default)]
    pub selected_subtitles: Vec<Value>,
    #[serde(default)]
    pub include_storyboards: bool,
    #[serde(default = "default_true")]
    pub include_artwork: bool,
    #[validate(length(min = 1, max = 128))]
    pub plan_revision: String,
    #[validate(length(min = 16, max = 128))]
    pub plan_hash: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CancelDownloadJobRequest {
    #[validate(length(max = 256))]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Validate)]
pub struct DeleteDownloadPackageRequest {
    #[serde(default)]
    pub delete_local_state: bool,
    #[validate(length(max = 256))]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct PackageTransferUrlsRequest {
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: String,
    #[validate(length(min = 1, max = 512))]
    pub file_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadPackageAccessQuery {
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadInventoryQuery {
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: Option<String>,
    #[validate(length(max = 64))]
    pub status: Option<String>,
    #[validate(range(min = 1, max = 100))]
    pub limit: Option<u32>,
    #[validate(length(max = 256))]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadSyncRequest {
    #[validate(length(min = 1, max = 128))]
    pub device_identifier: String,
    pub client_platform: DownloadClientPlatform,
    #[serde(default)]
    pub package_states: Vec<DownloadPackageStateUpdate>,
    #[serde(default)]
    pub playback_events: Vec<OfflinePlaybackEvent>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DownloadPackageStateUpdate {
    pub package_id: Uuid,
    pub local_status: DownloadLocalStatus,
    #[validate(range(min = 0))]
    pub bytes_downloaded: i64,
    #[validate(range(min = 0))]
    pub files_verified: i32,
    #[validate(length(max = 128))]
    pub local_manifest_hash_sha256: Option<String>,
    #[validate(range(min = 0))]
    pub local_resume_position_ms: i64,
    #[serde(default)]
    pub pending_events: Vec<Value>,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct OfflinePlaybackEvent {
    pub package_id: Uuid,
    #[validate(length(min = 1, max = 64))]
    pub event_type: String,
    #[validate(range(min = 0))]
    pub position_ms: i64,
    pub occurred_at: DateTime<Utc>,
    #[serde(default)]
    pub details: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadPlanResponse {
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub package_format: DownloadPackageFormat,
    pub package_strategy: String,
    pub quality_mode: DownloadQualityMode,
    pub target_resolution: Option<String>,
    pub target_bitrate_bps: Option<i64>,
    pub estimated_bytes: Option<i64>,
    pub estimated_duration_seconds: Option<i64>,
    pub source_file: Option<DownloadSourceFileResponse>,
    pub quality_options: Vec<DownloadQualityOptionResponse>,
    pub audio_options: Vec<Value>,
    pub subtitle_options: Vec<Value>,
    pub artwork_included: bool,
    pub storyboards_included: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub policy: Value,
    pub plan_revision: String,
    pub plan_hash: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadSourceFileResponse {
    pub id: Uuid,
    pub file_size: i64,
    pub container_format: String,
    pub video_codec: Option<String>,
    pub video_resolution: Option<String>,
    pub video_bitrate: Option<i32>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<i32>,
    pub audio_language: Option<String>,
    pub runtime_seconds: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadQualityOptionResponse {
    pub quality_mode: DownloadQualityMode,
    pub label: String,
    pub target_resolution: Option<String>,
    pub target_bitrate_bps: Option<i64>,
    pub estimated_bytes: Option<i64>,
    pub requires_transcode: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadJobResponse {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub device_identifier: String,
    pub status: DownloadJobStatus,
    pub package_format: DownloadPackageFormat,
    pub quality_mode: DownloadQualityMode,
    pub progress_percent: f32,
    pub bytes_expected: Option<i64>,
    pub bytes_prepared: i64,
    pub failure_reason: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadInventoryResponse {
    pub items: Vec<DownloadInventoryItemResponse>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadInventoryItemResponse {
    pub package_id: Uuid,
    pub job_id: Uuid,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub device_identifier: String,
    pub status: DownloadLocalStatus,
    pub package_format: DownloadPackageFormat,
    pub total_bytes: i64,
    pub bytes_downloaded: i64,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadPackageManifestResponse {
    pub package_id: Uuid,
    pub download_job_id: Uuid,
    pub schema_version: i32,
    pub manifest_version: i32,
    pub package_format: DownloadPackageFormat,
    pub package_strategy: String,
    pub media_item_id: Uuid,
    pub media_file_id: Option<Uuid>,
    pub source_version: Value,
    pub selected_quality: Value,
    pub total_bytes: i64,
    pub package_hash_sha256: Option<String>,
    pub files: Vec<DownloadPackageFileResponse>,
    pub selected_audio: Value,
    pub selected_subtitles: Vec<Value>,
    pub included_artwork: Value,
    pub included_storyboards: Value,
    pub expires_at: Option<DateTime<Utc>>,
    pub sync_metadata: Value,
    pub access_policy: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadPackageFileResponse {
    pub relative_path: String,
    pub file_role: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub checksum_sha256: String,
    pub segment_index: Option<i32>,
    pub is_required: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageTransferUrlsResponse {
    pub package_id: Uuid,
    pub expires_at: DateTime<Utc>,
    pub files: Vec<PackageTransferUrlResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackageTransferUrlResponse {
    pub relative_path: String,
    pub url: String,
    pub method: String,
    pub headers: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadSyncResponse {
    pub accepted_package_states: usize,
    pub accepted_playback_events: usize,
    pub revoked_package_ids: Vec<Uuid>,
    pub expired_package_ids: Vec<Uuid>,
    pub server_time: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadActionResponse {
    pub ok: bool,
    pub id: Uuid,
    pub status: String,
}

fn default_true() -> bool {
    true
}
