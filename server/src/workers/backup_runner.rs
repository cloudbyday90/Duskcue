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

use uuid::Uuid;

use crate::domains::backup::BackupError;
use crate::services::backup;
use crate::state::AppState;

pub async fn run_backup_database(
    state: &AppState,
    task_id: Uuid,
    _config: serde_json::Value,
) -> Result<(), BackupError> {
    tracing::info!(task_id = %task_id, "Starting scheduled database backup");

    let result = backup::run_scheduled_backup(state).await?;
    persist_run_stats(state, task_id, to_stats(&result)).await?;

    tracing::info!(
        task_id = %task_id,
        wal_g = result.wal_g.is_some(),
        pg_dump = result.pg_dump.is_some(),
        "Scheduled database backup completed"
    );

    Ok(())
}

pub async fn run_backup_verification(
    state: &AppState,
    task_id: Uuid,
    config: serde_json::Value,
) -> Result<(), BackupError> {
    tracing::info!(task_id = %task_id, "Starting scheduled backup verification");

    let backup_config = state.runtime_config.load().backup.clone();
    if !backup_config.verification_enabled {
        persist_run_stats(
            state,
            task_id,
            serde_json::json!({
                "status": "skipped",
                "reason": "backup verification is disabled"
            }),
        )
        .await?;
        tracing::info!(task_id = %task_id, "Scheduled backup verification skipped");
        return Ok(());
    }

    let verify_wal_g = config
        .get("verify_wal_g")
        .and_then(|value| value.as_bool())
        .unwrap_or(backup_config.wal_g_enabled);
    let verify_pg_dump = config
        .get("verify_pg_dump")
        .and_then(|value| value.as_bool())
        .unwrap_or(backup_config.pg_dump_enabled);

    let result = backup::verify_backups(state, verify_wal_g, verify_pg_dump, None).await?;
    persist_run_stats(state, task_id, to_stats(&result)).await?;

    tracing::info!(
        task_id = %task_id,
        wal_g = result.wal_g.is_some(),
        pg_dump = result.pg_dump.is_some(),
        "Scheduled backup verification completed"
    );

    Ok(())
}

pub async fn run_backup_retention_cleanup(
    state: &AppState,
    task_id: Uuid,
    _config: serde_json::Value,
) -> Result<(), BackupError> {
    tracing::info!(task_id = %task_id, "Starting scheduled backup retention cleanup");

    let result = backup::run_retention_cleanup(state).await?;
    persist_run_stats(state, task_id, to_stats(&result)).await?;

    tracing::info!(
        task_id = %task_id,
        wal_g = result.wal_g.is_some(),
        pg_dump_deleted = result.pg_dump_deleted,
        pg_dump_retained = result.pg_dump_retained,
        "Scheduled backup retention cleanup completed"
    );

    Ok(())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: serde_json::Value,
) -> Result<(), BackupError> {
    sqlx::query(
        r#"
        UPDATE scheduled_task_runs
        SET stats = $2
        WHERE scheduled_task_id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(stats)
    .execute(&state.pool)
    .await?;

    Ok(())
}

fn to_stats<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or_else(|err| {
        serde_json::json!({
            "status": "completed",
            "stats_error": err.to_string()
        })
    })
}
