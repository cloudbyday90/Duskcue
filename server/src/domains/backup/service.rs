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

use sqlx::Row;

use crate::services::backup as backup_coordinator;
use crate::state::{AppState, BackupConfig, WalGStorageType};

use super::error::BackupError;
use super::types::{
    BackupConfigResponse, BackupReadinessResponse, BackupRunListResponse, BackupRunResponse,
    BackupRunRow, BackupStatusResponse, BackupTaskListResponse, BackupTaskResponse, BackupTaskRow,
    BackupVerificationResponse, PgDumpTriggerResponse, PostgresSettingResponse,
    WalArchiveStatusResponse, WalGStatusCheckResponse,
};

pub async fn get_backup_status(state: &AppState) -> Result<BackupStatusResponse, BackupError> {
    let config = state.runtime_config.load().backup.clone();
    let postgres_settings = get_postgres_settings(state).await?;
    let wal_archive = get_wal_archive_status(state).await?;
    let tasks = get_backup_tasks(state).await?;
    let recent_runs = get_recent_backup_runs(state, 10).await?;
    let readiness = build_readiness(&config, &postgres_settings, wal_archive.as_ref(), &tasks);

    Ok(BackupStatusResponse {
        config: BackupConfigResponse::from(&config),
        readiness,
        postgres_settings,
        wal_archive,
        tasks,
        recent_runs,
    })
}

pub async fn list_backup_tasks(state: &AppState) -> Result<BackupTaskListResponse, BackupError> {
    Ok(BackupTaskListResponse {
        items: get_backup_tasks(state).await?,
    })
}

pub async fn list_backup_runs(
    state: &AppState,
    limit: u32,
) -> Result<BackupRunListResponse, BackupError> {
    Ok(BackupRunListResponse {
        items: get_recent_backup_runs(state, limit).await?,
        limit,
    })
}

pub async fn check_wal_g_status(state: &AppState) -> Result<WalGStatusCheckResponse, BackupError> {
    Ok(backup_coordinator::check_wal_g_status(state).await?.into())
}

pub async fn trigger_pg_dump(
    state: &AppState,
    label: Option<&str>,
    verify: bool,
) -> Result<PgDumpTriggerResponse, BackupError> {
    Ok(backup_coordinator::run_pg_dump(state, label, verify)
        .await?
        .into())
}

pub async fn verify_backups(
    state: &AppState,
    verify_wal_g: bool,
    verify_pg_dump: bool,
    pg_dump_path: Option<&str>,
) -> Result<BackupVerificationResponse, BackupError> {
    Ok(
        backup_coordinator::verify_backups(state, verify_wal_g, verify_pg_dump, pg_dump_path)
            .await?
            .into(),
    )
}

async fn get_postgres_settings(
    state: &AppState,
) -> Result<Vec<PostgresSettingResponse>, BackupError> {
    let rows = sqlx::query(
        "SELECT name, setting
         FROM pg_settings
         WHERE name IN (
             'fsync',
             'full_page_writes',
             'synchronous_commit',
             'data_checksums',
             'wal_level',
             'archive_mode',
             'archive_timeout'
         )
         ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let name: String = row.get("name");
            let setting: String = row.get("setting");
            let expected = expected_setting(&name).to_string();
            let ok = setting_matches(&name, &setting);

            PostgresSettingResponse {
                name,
                setting,
                expected,
                ok,
            }
        })
        .collect())
}

async fn get_wal_archive_status(
    state: &AppState,
) -> Result<Option<WalArchiveStatusResponse>, BackupError> {
    let row = sqlx::query(
        "SELECT archived_count,
                last_archived_wal,
                last_archived_time,
                failed_count,
                last_failed_wal,
                last_failed_time
         FROM pg_stat_archiver",
    )
    .fetch_optional(&state.pool)
    .await?;

    Ok(row.map(|row| WalArchiveStatusResponse {
        archived_count: row.get("archived_count"),
        last_archived_wal: row.get("last_archived_wal"),
        last_archived_time: row.get("last_archived_time"),
        failed_count: row.get("failed_count"),
        last_failed_wal: row.get("last_failed_wal"),
        last_failed_time: row.get("last_failed_time"),
    }))
}

async fn get_backup_tasks(state: &AppState) -> Result<Vec<BackupTaskResponse>, BackupError> {
    let rows = sqlx::query(
        "SELECT id,
                name,
                task_type,
                cron_expression,
                interval_seconds,
                is_enabled,
                state,
                consecutive_failures,
                last_run_at,
                last_run_result,
                last_error,
                next_run_at,
                config
         FROM scheduled_tasks
         WHERE task_type IN (
             'backup_database',
             'backup_verification',
             'database_integrity_check',
             'backup_retention_cleanup'
         )
         ORDER BY task_type",
    )
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            BackupTaskRow {
                id: row.get("id"),
                name: row.get("name"),
                task_type: row.get("task_type"),
                cron_expression: row.get("cron_expression"),
                interval_seconds: row.get("interval_seconds"),
                is_enabled: row.get("is_enabled"),
                state: row.get("state"),
                consecutive_failures: row.get("consecutive_failures"),
                last_run_at: row.get("last_run_at"),
                last_run_result: row.get("last_run_result"),
                last_error: row.get("last_error"),
                next_run_at: row.get("next_run_at"),
                config: row.get("config"),
            }
            .into()
        })
        .collect())
}

async fn get_recent_backup_runs(
    state: &AppState,
    limit: u32,
) -> Result<Vec<BackupRunResponse>, BackupError> {
    let rows = sqlx::query(
        "SELECT r.id,
                r.scheduled_task_id,
                t.name AS task_name,
                t.task_type,
                r.trigger_type,
                r.state,
                r.started_at,
                r.completed_at,
                r.duration_ms,
                r.result,
                r.error_message,
                r.stats
         FROM scheduled_task_runs r
         JOIN scheduled_tasks t ON t.id = r.scheduled_task_id
         WHERE t.task_type IN (
             'backup_database',
             'backup_verification',
             'database_integrity_check',
             'backup_retention_cleanup'
         )
         ORDER BY r.started_at DESC
         LIMIT $1",
    )
    .bind(i64::from(limit))
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            BackupRunRow {
                id: row.get("id"),
                scheduled_task_id: row.get("scheduled_task_id"),
                task_name: row.get("task_name"),
                task_type: row.get("task_type"),
                trigger_type: row.get("trigger_type"),
                state: row.get("state"),
                started_at: row.get("started_at"),
                completed_at: row.get("completed_at"),
                duration_ms: row.get("duration_ms"),
                result: row.get("result"),
                error_message: row.get("error_message"),
                stats: row.get("stats"),
            }
            .into()
        })
        .collect())
}

fn build_readiness(
    config: &BackupConfig,
    postgres_settings: &[PostgresSettingResponse],
    wal_archive: Option<&WalArchiveStatusResponse>,
    tasks: &[BackupTaskResponse],
) -> BackupReadinessResponse {
    if !config.wal_g_enabled && !config.pg_dump_enabled {
        return BackupReadinessResponse {
            status: "disabled".to_string(),
            issues: vec!["WAL-G and pg_dump backups are both disabled".to_string()],
        };
    }

    let mut issues = Vec::new();

    if config.wal_g_enabled {
        match config.wal_g_storage_type {
            WalGStorageType::Local if config.wal_g_storage_path.trim().is_empty() => {
                issues.push("WAL-G local storage path is empty".to_string());
            }
            WalGStorageType::S3 if config.wal_g_s3_bucket.trim().is_empty() => {
                issues.push("WAL-G S3 bucket is not configured".to_string());
            }
            _ => {}
        }

        if !has_setting_ok(postgres_settings, "archive_mode") {
            issues.push("PostgreSQL archive_mode is not on".to_string());
        }

        if wal_archive.is_none() {
            issues.push("PostgreSQL WAL archiver status is unavailable".to_string());
        }
    }

    if config.pg_dump_enabled && config.pg_dump_storage_path.trim().is_empty() {
        issues.push("pg_dump storage path is empty".to_string());
    }

    for name in [
        "fsync",
        "full_page_writes",
        "synchronous_commit",
        "wal_level",
    ] {
        if !has_setting_ok(postgres_settings, name) {
            issues.push(format!("PostgreSQL {name} setting is not recovery-safe"));
        }
    }

    if config.data_checksums && !has_setting_ok(postgres_settings, "data_checksums") {
        issues.push("PostgreSQL data_checksums is not on".to_string());
    }

    if config.wal_g_enabled && task_missing_or_disabled(tasks, "backup_database") {
        issues.push("backup_database scheduled task is missing or disabled".to_string());
    }

    if config.verification_enabled && task_missing_or_disabled(tasks, "backup_verification") {
        issues.push("backup_verification scheduled task is missing or disabled".to_string());
    }

    BackupReadinessResponse {
        status: if issues.is_empty() {
            "ready".to_string()
        } else {
            "degraded".to_string()
        },
        issues,
    }
}

fn expected_setting(name: &str) -> &'static str {
    match name {
        "archive_mode" => "on",
        "archive_timeout" => "60s or less",
        "data_checksums" => "on",
        "fsync" => "on",
        "full_page_writes" => "on",
        "synchronous_commit" => "on",
        "wal_level" => "replica or logical",
        _ => "configured",
    }
}

fn setting_matches(name: &str, setting: &str) -> bool {
    match name {
        "archive_mode" => setting == "on",
        "archive_timeout" => parse_seconds(setting).is_some_and(|seconds| seconds <= 60),
        "data_checksums" => setting == "on",
        "fsync" => setting == "on",
        "full_page_writes" => setting == "on",
        "synchronous_commit" => setting == "on",
        "wal_level" => setting == "replica" || setting == "logical",
        _ => true,
    }
}

fn parse_seconds(setting: &str) -> Option<u64> {
    if let Some(seconds) = setting.strip_suffix('s') {
        return seconds.parse().ok();
    }

    setting.parse().ok()
}

fn has_setting_ok(settings: &[PostgresSettingResponse], name: &str) -> bool {
    settings
        .iter()
        .find(|setting| setting.name == name)
        .is_some_and(|setting| setting.ok)
}

fn task_missing_or_disabled(tasks: &[BackupTaskResponse], task_type: &str) -> bool {
    tasks
        .iter()
        .find(|task| task.task_type == task_type)
        .is_none_or(|task| !task.is_enabled)
}

impl From<BackupTaskRow> for BackupTaskResponse {
    fn from(row: BackupTaskRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            task_type: row.task_type,
            cron_expression: row.cron_expression,
            interval_seconds: row.interval_seconds,
            is_enabled: row.is_enabled,
            state: row.state,
            consecutive_failures: row.consecutive_failures,
            last_run_at: row.last_run_at,
            last_run_result: row.last_run_result,
            last_error: row.last_error,
            next_run_at: row.next_run_at,
            config: row.config,
        }
    }
}

impl From<BackupRunRow> for BackupRunResponse {
    fn from(row: BackupRunRow) -> Self {
        Self {
            id: row.id,
            scheduled_task_id: row.scheduled_task_id,
            task_name: row.task_name,
            task_type: row.task_type,
            trigger_type: row.trigger_type,
            state: row.state,
            started_at: row.started_at,
            completed_at: row.completed_at,
            duration_ms: row.duration_ms,
            result: row.result,
            error_message: row.error_message,
            stats: row.stats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_seconds_accepts_plain_and_suffixed_seconds() {
        assert_eq!(parse_seconds("60"), Some(60));
        assert_eq!(parse_seconds("60s"), Some(60));
        assert_eq!(parse_seconds("1min"), None);
    }

    #[test]
    fn setting_matches_accepts_replica_or_logical_wal_level() {
        assert!(setting_matches("wal_level", "replica"));
        assert!(setting_matches("wal_level", "logical"));
        assert!(!setting_matches("wal_level", "minimal"));
    }

    #[test]
    fn disabled_config_reports_disabled() {
        let config = BackupConfig {
            wal_g_enabled: false,
            pg_dump_enabled: false,
            ..BackupConfig::default()
        };

        let readiness = build_readiness(&config, &[], None, &[]);

        assert_eq!(readiness.status, "disabled");
        assert_eq!(readiness.issues.len(), 1);
    }
}
