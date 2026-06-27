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

use crate::state::{BackupConfig, WalGStorageType};

#[derive(Debug, Clone, Serialize)]
pub struct BackupConfigResponse {
    pub wal_g_enabled: bool,
    pub wal_g_storage_type: WalGStorageType,
    pub wal_g_storage_path: String,
    pub wal_g_s3_endpoint: String,
    pub wal_g_s3_bucket_configured: bool,
    pub wal_g_s3_prefix: String,
    pub wal_g_s3_region: String,
    pub wal_g_encryption_enabled: bool,
    pub wal_g_encryption_key_configured: bool,
    pub wal_g_encryption_auto_s3: bool,
    pub wal_g_retention_full: u32,
    pub wal_g_retention_weekly: u32,
    pub wal_g_retention_monthly: u32,
    pub pg_dump_enabled: bool,
    pub pg_dump_storage_path: String,
    pub pg_dump_retention_daily: u32,
    pub pg_dump_retention_monthly: u32,
    pub archive_timeout_seconds: u32,
    pub data_checksums: bool,
    pub verification_enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupReadinessResponse {
    pub status: String,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PostgresSettingResponse {
    pub name: String,
    pub setting: String,
    pub expected: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct WalArchiveStatusResponse {
    pub archived_count: i64,
    pub last_archived_wal: Option<String>,
    pub last_archived_time: Option<DateTime<Utc>>,
    pub failed_count: i64,
    pub last_failed_wal: Option<String>,
    pub last_failed_time: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct BackupTaskRow {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i32>,
    pub is_enabled: bool,
    pub state: String,
    pub consecutive_failures: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_result: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupTaskResponse {
    pub id: Uuid,
    pub name: String,
    pub task_type: String,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i32>,
    pub is_enabled: bool,
    pub state: String,
    pub consecutive_failures: i32,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_run_result: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<DateTime<Utc>>,
    pub config: Value,
}

#[derive(Debug)]
pub struct BackupRunRow {
    pub id: Uuid,
    pub scheduled_task_id: Uuid,
    pub task_name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i32>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub stats: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupRunResponse {
    pub id: Uuid,
    pub scheduled_task_id: Uuid,
    pub task_name: String,
    pub task_type: String,
    pub trigger_type: String,
    pub state: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i32>,
    pub result: Option<String>,
    pub error_message: Option<String>,
    pub stats: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupStatusResponse {
    pub config: BackupConfigResponse,
    pub readiness: BackupReadinessResponse,
    pub postgres_settings: Vec<PostgresSettingResponse>,
    pub wal_archive: Option<WalArchiveStatusResponse>,
    pub tasks: Vec<BackupTaskResponse>,
    pub recent_runs: Vec<BackupRunResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupTaskListResponse {
    pub items: Vec<BackupTaskResponse>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupRunListResponse {
    pub items: Vec<BackupRunResponse>,
    pub limit: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackupRunsQuery {
    pub limit: Option<u32>,
}

impl From<&BackupConfig> for BackupConfigResponse {
    fn from(config: &BackupConfig) -> Self {
        let wal_g_encryption_enabled = config.wal_g_encryption_enabled
            || (config.wal_g_encryption_auto_s3
                && matches!(config.wal_g_storage_type, WalGStorageType::S3));

        Self {
            wal_g_enabled: config.wal_g_enabled,
            wal_g_storage_type: config.wal_g_storage_type.clone(),
            wal_g_storage_path: config.wal_g_storage_path.clone(),
            wal_g_s3_endpoint: config.wal_g_s3_endpoint.clone(),
            wal_g_s3_bucket_configured: !config.wal_g_s3_bucket.trim().is_empty(),
            wal_g_s3_prefix: config.wal_g_s3_prefix.clone(),
            wal_g_s3_region: config.wal_g_s3_region.clone(),
            wal_g_encryption_enabled,
            wal_g_encryption_key_configured: !config.wal_g_encryption_key_id.trim().is_empty(),
            wal_g_encryption_auto_s3: config.wal_g_encryption_auto_s3,
            wal_g_retention_full: config.wal_g_retention_full,
            wal_g_retention_weekly: config.wal_g_retention_weekly,
            wal_g_retention_monthly: config.wal_g_retention_monthly,
            pg_dump_enabled: config.pg_dump_enabled,
            pg_dump_storage_path: config.pg_dump_storage_path.clone(),
            pg_dump_retention_daily: config.pg_dump_retention_daily,
            pg_dump_retention_monthly: config.pg_dump_retention_monthly,
            archive_timeout_seconds: config.archive_timeout_seconds,
            data_checksums: config.data_checksums,
            verification_enabled: config.verification_enabled,
        }
    }
}
