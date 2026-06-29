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

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::Serialize;
use serde_json::Value;
use sqlx::Row;
use tokio::fs;
use uuid::Uuid;

use crate::state::AppState;

const DEFAULT_DELETE_PLEX_UPLOADS_AFTER_HOURS: i64 = 24;
const DEFAULT_DELETE_FAILED_TEMP_FILES_AFTER_HOURS: i64 = 24;
const DEFAULT_DELETE_COMPLETED_SOURCES_AFTER_DAYS: i64 = 90;
const DEFAULT_DELETE_IMPORT_LOGS_AFTER_DAYS: i64 = 90;

#[derive(Debug, thiserror::Error)]
pub enum MigrationCleanupError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to serialize cleanup stats: {0}")]
    StatsSerialization(#[from] serde_json::Error),
    #[error("Migration cleanup completed with {failed_count} file cleanup error(s)")]
    CleanupFailures { failed_count: usize },
}

#[derive(Debug, Clone, Copy)]
struct MigrationCleanupConfig {
    delete_plex_uploads_after_hours: i64,
    delete_failed_temp_files_after_hours: i64,
    delete_completed_sources_after_days: i64,
    delete_import_logs_after_days: i64,
}

#[derive(Debug, Serialize)]
struct MigrationCleanupStats {
    status: String,
    upload_root: String,
    delete_plex_uploads_after_hours: i64,
    delete_failed_temp_files_after_hours: i64,
    delete_completed_sources_after_days: i64,
    delete_import_logs_after_days: i64,
    completed_plex_uploads_selected: usize,
    completed_upload_dirs_deleted: usize,
    completed_upload_dirs_missing: usize,
    completed_upload_delete_errors: usize,
    stale_temp_files_selected: usize,
    stale_temp_files_deleted: usize,
    stale_temp_files_missing: usize,
    stale_temp_delete_errors: usize,
    import_logs_deleted: u64,
    completed_sources_deleted: u64,
    errors: Vec<String>,
}

enum DeleteOutcome {
    Deleted,
    Missing,
}

impl MigrationCleanupConfig {
    fn from_value(value: &Value) -> Self {
        Self {
            delete_plex_uploads_after_hours: read_retention(
                value,
                "delete_plex_uploads_after_hours",
                DEFAULT_DELETE_PLEX_UPLOADS_AFTER_HOURS,
                1,
                24 * 365 * 10,
            ),
            delete_failed_temp_files_after_hours: read_retention(
                value,
                "delete_failed_temp_files_after_hours",
                DEFAULT_DELETE_FAILED_TEMP_FILES_AFTER_HOURS,
                1,
                24 * 365 * 10,
            ),
            delete_completed_sources_after_days: read_retention(
                value,
                "delete_completed_sources_after_days",
                DEFAULT_DELETE_COMPLETED_SOURCES_AFTER_DAYS,
                1,
                365 * 10,
            ),
            delete_import_logs_after_days: read_retention(
                value,
                "delete_import_logs_after_days",
                DEFAULT_DELETE_IMPORT_LOGS_AFTER_DAYS,
                1,
                365 * 10,
            ),
        }
    }
}

impl MigrationCleanupStats {
    fn new(config: MigrationCleanupConfig, upload_root: &Path) -> Self {
        Self {
            status: "running".to_string(),
            upload_root: upload_root.to_string_lossy().to_string(),
            delete_plex_uploads_after_hours: config.delete_plex_uploads_after_hours,
            delete_failed_temp_files_after_hours: config.delete_failed_temp_files_after_hours,
            delete_completed_sources_after_days: config.delete_completed_sources_after_days,
            delete_import_logs_after_days: config.delete_import_logs_after_days,
            completed_plex_uploads_selected: 0,
            completed_upload_dirs_deleted: 0,
            completed_upload_dirs_missing: 0,
            completed_upload_delete_errors: 0,
            stale_temp_files_selected: 0,
            stale_temp_files_deleted: 0,
            stale_temp_files_missing: 0,
            stale_temp_delete_errors: 0,
            import_logs_deleted: 0,
            completed_sources_deleted: 0,
            errors: Vec::new(),
        }
    }
}

pub async fn run_migration_cleanup(
    state: &AppState,
    task_id: Uuid,
    config: Value,
) -> Result<(), MigrationCleanupError> {
    let config = MigrationCleanupConfig::from_value(&config);
    let upload_root = migration_upload_root(state);
    let mut stats = MigrationCleanupStats::new(config, &upload_root);
    let mut failed_completed_upload_ids = Vec::new();

    tracing::info!(task_id = %task_id, "Starting scheduled migration cleanup");

    let completed_plex_sources =
        fetch_completed_plex_sources(state, config.delete_plex_uploads_after_hours).await?;
    stats.completed_plex_uploads_selected = completed_plex_sources.len();

    for source_id in completed_plex_sources {
        match remove_migration_upload_dir(&upload_root, source_id).await {
            Ok(DeleteOutcome::Deleted) => {
                stats.completed_upload_dirs_deleted += 1;
                mark_plex_upload_cleaned(state, source_id, task_id).await?;
            }
            Ok(DeleteOutcome::Missing) => {
                stats.completed_upload_dirs_missing += 1;
                mark_plex_upload_cleaned(state, source_id, task_id).await?;
            }
            Err(error) => {
                stats.completed_upload_delete_errors += 1;
                failed_completed_upload_ids.push(source_id);
                stats.errors.push(format!(
                    "failed to delete Plex upload directory for {source_id}: {error}"
                ));
            }
        }
    }

    let stale_failed_sources =
        fetch_stale_failed_plex_sources(state, config.delete_failed_temp_files_after_hours).await?;
    stats.stale_temp_files_selected = stale_failed_sources.len();
    let mut attempted_temp_sources = HashSet::new();

    for source_id in stale_failed_sources {
        attempted_temp_sources.insert(source_id);
        match remove_stale_temp_file(&upload_root, source_id).await {
            Ok(DeleteOutcome::Deleted) => stats.stale_temp_files_deleted += 1,
            Ok(DeleteOutcome::Missing) => stats.stale_temp_files_missing += 1,
            Err(error) => {
                stats.stale_temp_delete_errors += 1;
                stats.errors.push(format!(
                    "failed to delete stale Plex temp file for {source_id}: {error}"
                ));
            }
        }
    }

    cleanup_orphaned_temp_files(
        &upload_root,
        config.delete_failed_temp_files_after_hours,
        &attempted_temp_sources,
        &mut stats,
    )
    .await;

    stats.import_logs_deleted =
        prune_old_import_logs(state, config.delete_import_logs_after_days).await?;
    stats.completed_sources_deleted = prune_old_completed_sources(
        state,
        config.delete_completed_sources_after_days,
        &failed_completed_upload_ids,
    )
    .await?;

    if stats.errors.is_empty() {
        stats.status = "completed".to_string();
        persist_run_stats(state, task_id, &stats).await?;
        tracing::info!(
            task_id = %task_id,
            completed_upload_dirs_deleted = stats.completed_upload_dirs_deleted,
            stale_temp_files_deleted = stats.stale_temp_files_deleted,
            import_logs_deleted = stats.import_logs_deleted,
            completed_sources_deleted = stats.completed_sources_deleted,
            "Scheduled migration cleanup completed"
        );
        Ok(())
    } else {
        stats.status = "failed".to_string();
        let failed_count = stats.completed_upload_delete_errors + stats.stale_temp_delete_errors;
        persist_run_stats(state, task_id, &stats).await?;
        tracing::warn!(
            task_id = %task_id,
            failed_count,
            "Scheduled migration cleanup completed with file cleanup errors"
        );
        Err(MigrationCleanupError::CleanupFailures { failed_count })
    }
}

async fn fetch_completed_plex_sources(
    state: &AppState,
    retention_hours: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id
        FROM migration_sources
        WHERE platform = 'plex'
          AND status = 'completed'
          AND COALESCE(last_run_at, created_at) < now() - ($1::INT * INTERVAL '1 hour')
        "#,
    )
    .bind(retention_hours as i32)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows.iter().map(|row| row.get("id")).collect())
}

async fn fetch_stale_failed_plex_sources(
    state: &AppState,
    retention_hours: i64,
) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id
        FROM migration_sources
        WHERE platform = 'plex'
          AND status IN ('failed', 'cancelled')
          AND COALESCE(last_run_at, created_at) < now() - ($1::INT * INTERVAL '1 hour')
        "#,
    )
    .bind(retention_hours as i32)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows.iter().map(|row| row.get("id")).collect())
}

async fn mark_plex_upload_cleaned(
    state: &AppState,
    source_id: Uuid,
    task_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE migration_sources
        SET connection_config =
            (connection_config - 'stored_path') ||
            jsonb_build_object(
                'plex_upload_deleted_at', now(),
                'plex_upload_cleanup_task_id', $2::TEXT
            )
        WHERE id = $1
        "#,
    )
    .bind(source_id)
    .bind(task_id.to_string())
    .execute(&state.pool)
    .await?;

    Ok(())
}

async fn prune_old_import_logs(state: &AppState, retention_days: i64) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM migration_import_log log
        USING migration_sources source
        WHERE log.migration_source_id = source.id
          AND source.status NOT IN ('discovering', 'matching', 'importing')
          AND log.created_at < now() - ($1::INT * INTERVAL '1 day')
        "#,
    )
    .bind(retention_days as i32)
    .execute(&state.pool)
    .await?;

    Ok(result.rows_affected())
}

async fn prune_old_completed_sources(
    state: &AppState,
    retention_days: i64,
    failed_plex_upload_ids: &[Uuid],
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        r#"
        DELETE FROM migration_sources
        WHERE status = 'completed'
          AND COALESCE(last_run_at, created_at) < now() - ($1::INT * INTERVAL '1 day')
          AND NOT (platform = 'plex' AND id = ANY($2::UUID[]))
        "#,
    )
    .bind(retention_days as i32)
    .bind(failed_plex_upload_ids)
    .execute(&state.pool)
    .await?;

    Ok(result.rows_affected())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &MigrationCleanupStats,
) -> Result<(), MigrationCleanupError> {
    sqlx::query(
        r#"
        UPDATE scheduled_task_runs
        SET stats = $2
        WHERE scheduled_task_id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(serde_json::to_value(stats)?)
    .execute(&state.pool)
    .await?;

    Ok(())
}

async fn cleanup_orphaned_temp_files(
    upload_root: &Path,
    retention_hours: i64,
    attempted_sources: &HashSet<Uuid>,
    stats: &mut MigrationCleanupStats,
) {
    let Ok(mut entries) = fs::read_dir(upload_root).await else {
        return;
    };
    let retention = Duration::from_secs((retention_hours as u64).saturating_mul(60 * 60));

    while let Ok(Some(entry)) = entries.next_entry().await {
        let Some(source_id) = uuid_from_file_name(&entry.path()) else {
            continue;
        };
        if attempted_sources.contains(&source_id) {
            continue;
        }

        let temp_path = temp_upload_path(upload_root, source_id);
        let Ok(metadata) = fs::metadata(&temp_path).await else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if !is_older_than(modified, retention) {
            continue;
        }

        stats.stale_temp_files_selected += 1;
        match remove_stale_temp_file(upload_root, source_id).await {
            Ok(DeleteOutcome::Deleted) => stats.stale_temp_files_deleted += 1,
            Ok(DeleteOutcome::Missing) => stats.stale_temp_files_missing += 1,
            Err(error) => {
                stats.stale_temp_delete_errors += 1;
                stats.errors.push(format!(
                    "failed to delete orphaned Plex temp file for {source_id}: {error}"
                ));
            }
        }
    }
}

async fn remove_migration_upload_dir(
    upload_root: &Path,
    source_id: Uuid,
) -> io::Result<DeleteOutcome> {
    let dir = migration_upload_dir(upload_root, source_id);
    match fs::metadata(&dir).await {
        Ok(metadata) if metadata.is_dir() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration upload path is not a directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(DeleteOutcome::Missing),
        Err(error) => return Err(error),
    }

    ensure_safe_uuid_child(upload_root, &dir, source_id).await?;
    fs::remove_dir_all(&dir).await?;
    Ok(DeleteOutcome::Deleted)
}

async fn remove_stale_temp_file(upload_root: &Path, source_id: Uuid) -> io::Result<DeleteOutcome> {
    let path = temp_upload_path(upload_root, source_id);
    match fs::metadata(&path).await {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "migration temp upload path is not a file",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(DeleteOutcome::Missing),
        Err(error) => return Err(error),
    }

    ensure_safe_uuid_child(upload_root, &path, source_id).await?;
    fs::remove_file(&path).await?;
    Ok(DeleteOutcome::Deleted)
}

async fn ensure_safe_uuid_child(
    upload_root: &Path,
    path: &Path,
    source_id: Uuid,
) -> io::Result<()> {
    let canonical_root = fs::canonicalize(upload_root).await?;
    let canonical_path = fs::canonicalize(path).await?;
    let expected_dir = migration_upload_dir(&canonical_root, source_id);

    if !canonical_path.starts_with(&canonical_root) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "migration cleanup path is outside the upload root",
        ));
    }

    if !canonical_path.starts_with(expected_dir) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "migration cleanup path is outside the source upload directory",
        ));
    }

    Ok(())
}

fn read_retention(value: &Value, key: &str, default: i64, min: i64, max: i64) -> i64 {
    value
        .get(key)
        .and_then(Value::as_i64)
        .unwrap_or(default)
        .clamp(min, max)
}

fn migration_upload_root(state: &AppState) -> PathBuf {
    state.bootstrap.data_dir.join("migrations")
}

fn migration_upload_dir(upload_root: &Path, source_id: Uuid) -> PathBuf {
    upload_root.join(source_id.to_string())
}

fn temp_upload_path(upload_root: &Path, source_id: Uuid) -> PathBuf {
    migration_upload_dir(upload_root, source_id).join("plex.db.uploading")
}

fn uuid_from_file_name(path: &Path) -> Option<Uuid> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| Uuid::parse_str(name).ok())
}

fn is_older_than(modified: SystemTime, retention: Duration) -> bool {
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age >= retention)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, SystemTime};

    use serde_json::json;
    use uuid::Uuid;

    use super::{MigrationCleanupConfig, is_older_than, uuid_from_file_name};

    #[test]
    fn config_uses_documented_defaults() {
        let config = MigrationCleanupConfig::from_value(&json!({}));

        assert_eq!(config.delete_plex_uploads_after_hours, 24);
        assert_eq!(config.delete_failed_temp_files_after_hours, 24);
        assert_eq!(config.delete_completed_sources_after_days, 90);
        assert_eq!(config.delete_import_logs_after_days, 90);
    }

    #[test]
    fn config_clamps_retention_values() {
        let config = MigrationCleanupConfig::from_value(&json!({
            "delete_plex_uploads_after_hours": 0,
            "delete_failed_temp_files_after_hours": 999_999,
            "delete_completed_sources_after_days": -10,
            "delete_import_logs_after_days": 999_999
        }));

        assert_eq!(config.delete_plex_uploads_after_hours, 1);
        assert_eq!(config.delete_failed_temp_files_after_hours, 87_600);
        assert_eq!(config.delete_completed_sources_after_days, 1);
        assert_eq!(config.delete_import_logs_after_days, 3_650);
    }

    #[test]
    fn uuid_file_name_parser_rejects_non_uuid_paths() {
        let id = Uuid::now_v7();

        assert_eq!(uuid_from_file_name(id.to_string().as_ref()), Some(id));
        assert_eq!(uuid_from_file_name("not-a-uuid".as_ref()), None);
    }

    #[test]
    fn older_than_handles_future_times_as_not_old() {
        let future = SystemTime::now() + Duration::from_secs(60);

        assert!(!is_older_than(future, Duration::from_secs(1)));
    }
}
