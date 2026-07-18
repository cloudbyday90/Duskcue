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

use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

const PARENT_TABLES: &[(&str, &str)] = &[
    ("play_sessions", "ANALYZE play_sessions"),
    ("play_events", "ANALYZE play_events"),
    ("audit_log", "ANALYZE audit_log"),
];

#[derive(Debug, thiserror::Error)]
pub enum AnalyzeParentsError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to serialize parent-analysis stats: {0}")]
    StatsSerialization(serde_json::Error),
    #[error("{failed_count} parent table analysis operation(s) failed")]
    AnalysisFailures { failed_count: usize },
}

#[derive(Debug, Clone, Serialize)]
struct AnalyzeParentsStats {
    status: String,
    enabled: bool,
    parents_analyzed: usize,
    failed_count: usize,
    parents: Vec<AnalyzeParentRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct AnalyzeParentRecord {
    parent_table: String,
    action: String,
    error: Option<String>,
}

pub async fn run_analyze_parents(
    state: &AppState,
    task_id: Uuid,
    task_config: serde_json::Value,
) -> Result<(), AnalyzeParentsError> {
    let enabled = task_enabled(
        &task_config,
        state
            .runtime_config
            .load()
            .maintenance
            .analyze_parent_tables_enabled,
    );
    if !enabled {
        let stats = AnalyzeParentsStats {
            status: "skipped".to_string(),
            enabled: false,
            parents_analyzed: 0,
            failed_count: 0,
            parents: Vec::new(),
        };
        persist_run_stats(state, task_id, &stats).await?;
        tracing::info!(task_id = %task_id, "Partitioned parent analysis skipped");
        return Ok(());
    }

    tracing::info!(task_id = %task_id, "Starting partitioned parent analysis");
    let mut parents = Vec::with_capacity(PARENT_TABLES.len());
    let mut parents_analyzed = 0usize;
    let mut failed_count = 0usize;

    for (parent_table, statement) in PARENT_TABLES {
        match sqlx::query(*statement).execute(&state.pool).await {
            Ok(_) => {
                parents_analyzed += 1;
                metrics::counter!(
                    "maintenance_parent_analyze_total",
                    "parent_table" => *parent_table
                )
                .increment(1);
                parents.push(AnalyzeParentRecord {
                    parent_table: (*parent_table).to_string(),
                    action: "analyzed".to_string(),
                    error: None,
                });
            }
            Err(error) => {
                failed_count += 1;
                let error = error.to_string();
                metrics::counter!(
                    "maintenance_parent_analyze_failures_total",
                    "parent_table" => *parent_table
                )
                .increment(1);
                tracing::error!(
                    task_id = %task_id,
                    parent_table,
                    error = %error,
                    "Failed to analyze partitioned parent table"
                );
                parents.push(AnalyzeParentRecord {
                    parent_table: (*parent_table).to_string(),
                    action: "failed".to_string(),
                    error: Some(error),
                });
            }
        }
    }

    let stats = AnalyzeParentsStats {
        status: if failed_count == 0 {
            "completed".to_string()
        } else {
            "failed".to_string()
        },
        enabled: true,
        parents_analyzed,
        failed_count,
        parents,
    };
    persist_run_stats(state, task_id, &stats).await?;
    if failed_count > 0 {
        return Err(AnalyzeParentsError::AnalysisFailures { failed_count });
    }

    tracing::info!(
        task_id = %task_id,
        parents_analyzed = stats.parents_analyzed,
        "Partitioned parent analysis completed"
    );
    Ok(())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &AnalyzeParentsStats,
) -> Result<(), AnalyzeParentsError> {
    let stats = serde_json::to_value(stats).map_err(AnalyzeParentsError::StatsSerialization)?;
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

fn task_enabled(task_config: &serde_json::Value, default: bool) -> bool {
    task_config
        .get("enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::task_enabled;

    #[test]
    fn task_config_overrides_maintenance_default() {
        assert!(task_enabled(&json!({ "enabled": true }), false));
        assert!(!task_enabled(&json!({ "enabled": false }), true));
    }

    #[test]
    fn invalid_or_missing_task_config_uses_maintenance_default() {
        assert!(task_enabled(&json!({}), true));
        assert!(!task_enabled(&json!({ "enabled": "yes" }), false));
    }
}
