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

//! Scheduler adapter for the recovery drill. Mirrors the
//! `workers::backup_runner` ↔ `services::backup` split: this module owns
//! scheduler integration (run row lifecycle, stats persistence, fallible
//! executor mapping); [`services::recovery_drill`] owns the actual restore
//! + structural check logic.
//!
//! Scheduled task type: `backup_recovery_drill`. Enabled by default
//! (Sundays 07:00) but a no-op when Docker is unavailable on the host.

use uuid::Uuid;

use crate::domains::backup::BackupError;
use crate::services::recovery_drill::{self, DrillOptions, RecoveryDrillStats};
use crate::state::AppState;

/// Scheduler entry point. Resolves [`DrillOptions`] from the task config and
/// delegates to [`recovery_drill::run_recovery_drill`]. The outcome is
/// mapped to the scheduler's fallible executor contract:
///
/// - Infrastructure failures (`BackupError` variants other than
///   `InvalidConfig`/`OperationInProgress`) bubble up as task failures.
/// - Drill-level failures (restore failed, structural check failed,
///   Docker unavailable) are persisted as stats with `status = failed` /
///   `unavailable` and the task completes successfully — the run is
///   "operationally successful" even if the drill found a problem. This
///   matches `disk_space_check`'s "threshold breach is a finding, not a
///   failure" semantics.
pub async fn run_recovery_drill(
    state: &AppState,
    task_id: Uuid,
    task_config: serde_json::Value,
) -> Result<(), BackupError> {
    tracing::info!(task_id = %task_id, "Starting recovery drill");

    let options = DrillOptions::from_state_and_task(state, &task_config);
    let stats = recovery_drill::run_recovery_drill(state, &options).await?;

    persist_run_stats(state, task_id, &stats).await?;

    tracing::info!(
        task_id = %task_id,
        status = %stats.status,
        duration_ms = stats.duration_ms,
        errors = stats.errors.len(),
        "Recovery drill completed"
    );

    Ok(())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &RecoveryDrillStats,
) -> Result<(), BackupError> {
    let value = serde_json::to_value(stats).map_err(|err| {
        BackupError::InvalidConfig(format!("failed to serialize recovery drill stats: {err}"))
    })?;

    sqlx::query(
        r#"
        UPDATE scheduled_task_runs
        SET stats = $2
        WHERE scheduled_task_id = $1
          AND state = 'running'
        "#,
    )
    .bind(task_id)
    .bind(value)
    .execute(&state.pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::recovery_drill::DisposalReport;
    use serde_json::json;

    #[test]
    fn skipped_stats_serialize_to_expected_shape() {
        let stats = RecoveryDrillStats {
            status: "skipped".to_string(),
            started_at: "2026-06-27T03:00:00+00:00".to_string(),
            completed_at: "2026-06-27T03:00:01+00:00".to_string(),
            duration_ms: 12,
            skip_reason: Some("pg_dump disabled".to_string()),
            disposable_postgres: None,
            backup_source: None,
            restore: None,
            structural_checks: Vec::new(),
            disposal: DisposalReport {
                status: "not_started".to_string(),
                stderr: None,
            },
            errors: Vec::new(),
        };
        let value = serde_json::to_value(&stats).unwrap();
        assert_eq!(value["status"], "skipped");
        assert_eq!(value["skip_reason"], "pg_dump disabled");
        assert!(value.get("disposable_postgres").is_none());
        assert!(value.get("backup_source").is_none());
        assert!(value.get("restore").is_none());
    }

    #[test]
    fn task_config_with_overrides_parses_into_expected_options() {
        let config = json!({
            "port": 6000,
            "keep_alive": true,
            "restore_jobs": 3
        });
        let options = DrillOptions::from_config_and_task(&Default::default(), &config);
        assert_eq!(options.port, 6000);
        assert!(options.keep_alive);
        assert_eq!(options.restore_jobs, 3);
    }
}
