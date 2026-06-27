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
use sqlx::Row;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, thiserror::Error)]
pub enum ReindexMaintenanceError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("{failed_count} index reindex operation(s) failed")]
    ReindexFailures { failed_count: usize },
    #[error("Failed to serialize reindex maintenance stats: {0}")]
    StatsSerialization(serde_json::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct ReindexMaintenanceStats {
    status: String,
    enabled: bool,
    bloat_threshold_percent: f64,
    min_index_size_mb: u32,
    min_index_size_bytes: i64,
    candidates_found: usize,
    reindexed_count: usize,
    failed_count: usize,
    skipped_expected_fillfactor_count: usize,
    candidates: Vec<IndexMaintenanceRecord>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexMaintenanceRecord {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_size_bytes: i64,
    avg_leaf_density_percent: f64,
    bloat_percent: f64,
    table_fillfactor: Option<u16>,
    action: String,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct IndexCandidate {
    schema_name: String,
    table_name: String,
    index_name: String,
    index_size_bytes: i64,
    avg_leaf_density_percent: f64,
    bloat_percent: f64,
    table_fillfactor: Option<u16>,
}

#[derive(Debug, Clone, Copy)]
struct ReindexConfig {
    enabled: bool,
    bloat_threshold_percent: f64,
    min_index_size_mb: u32,
}

impl ReindexConfig {
    fn from_state_and_task(state: &AppState, task_config: &serde_json::Value) -> Self {
        let maintenance = state.runtime_config.load().maintenance.clone();
        let enabled = task_config
            .get("enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(maintenance.reindex_enabled);

        let bloat_threshold_percent = task_config
            .get("bloat_threshold_percent")
            .and_then(|value| value.as_f64())
            .unwrap_or(maintenance.reindex_bloat_threshold_percent as f64)
            .clamp(10.0, 90.0);

        let min_index_size_mb = task_config
            .get("min_index_size_mb")
            .and_then(|value| value.as_u64())
            .map(|value| value as u32)
            .unwrap_or(maintenance.reindex_min_index_size_mb)
            .clamp(1, 1024);

        Self {
            enabled,
            bloat_threshold_percent,
            min_index_size_mb,
        }
    }

    fn min_index_size_bytes(self) -> i64 {
        self.min_index_size_mb as i64 * 1024 * 1024
    }
}

pub async fn run_reindex_maintenance(
    state: &AppState,
    task_id: Uuid,
    task_config: serde_json::Value,
) -> Result<(), ReindexMaintenanceError> {
    tracing::info!(task_id = %task_id, "Starting reindex maintenance");

    let config = ReindexConfig::from_state_and_task(state, &task_config);
    if !config.enabled {
        let stats = ReindexMaintenanceStats {
            status: "skipped".to_string(),
            enabled: false,
            bloat_threshold_percent: config.bloat_threshold_percent,
            min_index_size_mb: config.min_index_size_mb,
            min_index_size_bytes: config.min_index_size_bytes(),
            candidates_found: 0,
            reindexed_count: 0,
            failed_count: 0,
            skipped_expected_fillfactor_count: 0,
            candidates: Vec::new(),
        };
        persist_run_stats(state, task_id, &stats).await?;
        tracing::info!(task_id = %task_id, "Reindex maintenance skipped");
        return Ok(());
    }

    let candidates = find_reindex_candidates(state, config).await?;
    let mut records = Vec::with_capacity(candidates.len());
    let mut failed_count = 0usize;
    let mut reindexed_count = 0usize;
    let mut skipped_expected_fillfactor_count = 0usize;

    for candidate in candidates {
        if is_expected_fillfactor_space(&candidate) {
            skipped_expected_fillfactor_count += 1;
            records.push(candidate.into_record("skipped_expected_fillfactor", None));
            continue;
        }

        match reindex_index_concurrently(state, &candidate).await {
            Ok(()) => {
                reindexed_count += 1;
                metrics::counter!(
                    "maintenance_reindex_total",
                    "table" => candidate.table_name.clone(),
                    "index" => candidate.index_name.clone()
                )
                .increment(1);
                metrics::gauge!(
                    "maintenance_reindex_bloat_before",
                    "table" => candidate.table_name.clone(),
                    "index" => candidate.index_name.clone()
                )
                .set(candidate.bloat_percent);
                records.push(candidate.into_record("reindexed", None));
            }
            Err(err) => {
                failed_count += 1;
                let error = err.to_string();
                tracing::error!(
                    task_id = %task_id,
                    schema = %candidate.schema_name,
                    table = %candidate.table_name,
                    index = %candidate.index_name,
                    error = %error,
                    "Failed to reindex candidate"
                );
                records.push(candidate.into_record("failed", Some(error)));
            }
        }
    }

    let stats = ReindexMaintenanceStats {
        status: if failed_count == 0 {
            "completed".to_string()
        } else {
            "failed".to_string()
        },
        enabled: true,
        bloat_threshold_percent: config.bloat_threshold_percent,
        min_index_size_mb: config.min_index_size_mb,
        min_index_size_bytes: config.min_index_size_bytes(),
        candidates_found: records.len(),
        reindexed_count,
        failed_count,
        skipped_expected_fillfactor_count,
        candidates: records,
    };

    persist_run_stats(state, task_id, &stats).await?;

    if failed_count > 0 {
        return Err(ReindexMaintenanceError::ReindexFailures { failed_count });
    }

    tracing::info!(
        task_id = %task_id,
        candidates = stats.candidates_found,
        reindexed = stats.reindexed_count,
        skipped_expected_fillfactor = stats.skipped_expected_fillfactor_count,
        "Reindex maintenance completed"
    );

    Ok(())
}

async fn find_reindex_candidates(
    state: &AppState,
    config: ReindexConfig,
) -> Result<Vec<IndexCandidate>, ReindexMaintenanceError> {
    let rows = sqlx::query(
        r#"
        SELECT
            ns.nspname AS schema_name,
            tbl.relname AS table_name,
            idx.relname AS index_name,
            pg_relation_size(idx.oid)::BIGINT AS index_size_bytes,
            stat.avg_leaf_density::DOUBLE PRECISION AS avg_leaf_density_percent,
            (100.0 - stat.avg_leaf_density)::DOUBLE PRECISION AS bloat_percent,
            tbl.reloptions AS table_options
        FROM pg_stat_user_indexes sui
        JOIN pg_class idx ON idx.oid = sui.indexrelid
        JOIN pg_class tbl ON tbl.oid = sui.relid
        JOIN pg_namespace ns ON ns.oid = idx.relnamespace
        JOIN pg_am am ON am.oid = idx.relam
        JOIN pg_index pi ON pi.indexrelid = idx.oid
        CROSS JOIN LATERAL pgstatindex(idx.oid::regclass) AS stat
        WHERE ns.nspname = 'public'
          AND idx.relkind = 'i'
          AND tbl.relkind IN ('r', 'p')
          AND am.amname = 'btree'
          AND pi.indisvalid = true
          AND pi.indisready = true
          AND pg_relation_size(idx.oid) >= $1
          AND (100.0 - stat.avg_leaf_density) >= $2
          AND NOT EXISTS (
              SELECT 1
              FROM pg_constraint con
              WHERE con.conindid = idx.oid
                AND con.contype = 'x'
          )
        ORDER BY (100.0 - stat.avg_leaf_density) DESC,
                 pg_relation_size(idx.oid) DESC
        "#,
    )
    .bind(config.min_index_size_bytes())
    .bind(config.bloat_threshold_percent)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let table_options: Option<Vec<String>> = row.try_get("table_options").ok().flatten();
            IndexCandidate {
                schema_name: row.get("schema_name"),
                table_name: row.get("table_name"),
                index_name: row.get("index_name"),
                index_size_bytes: row.get("index_size_bytes"),
                avg_leaf_density_percent: row.get("avg_leaf_density_percent"),
                bloat_percent: row.get("bloat_percent"),
                table_fillfactor: table_options.as_deref().and_then(extract_table_fillfactor),
            }
        })
        .collect())
}

async fn reindex_index_concurrently(
    state: &AppState,
    candidate: &IndexCandidate,
) -> Result<(), sqlx::Error> {
    let qualified_index = format!(
        "{}.{}",
        quote_identifier(&candidate.schema_name),
        quote_identifier(&candidate.index_name)
    );
    let sql = format!("REINDEX INDEX CONCURRENTLY {qualified_index}");
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(&state.pool)
        .await?;
    Ok(())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &ReindexMaintenanceStats,
) -> Result<(), ReindexMaintenanceError> {
    let stats = serde_json::to_value(stats).map_err(ReindexMaintenanceError::StatsSerialization)?;
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

fn is_expected_fillfactor_space(candidate: &IndexCandidate) -> bool {
    let Some(fillfactor) = candidate.table_fillfactor else {
        return false;
    };
    if fillfactor >= 100 {
        return false;
    }
    candidate.bloat_percent <= (100 - fillfactor) as f64
}

fn extract_table_fillfactor(options: &[String]) -> Option<u16> {
    options.iter().find_map(|option| {
        option
            .strip_prefix("fillfactor=")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|value| (10..=100).contains(value))
    })
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

impl IndexCandidate {
    fn into_record(self, action: &str, error: Option<String>) -> IndexMaintenanceRecord {
        IndexMaintenanceRecord {
            schema_name: self.schema_name,
            table_name: self.table_name,
            index_name: self.index_name,
            index_size_bytes: self.index_size_bytes,
            avg_leaf_density_percent: self.avg_leaf_density_percent,
            bloat_percent: self.bloat_percent,
            table_fillfactor: self.table_fillfactor,
            action: action.to_string(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_table_fillfactor, quote_identifier};

    #[test]
    fn quotes_postgres_identifiers() {
        assert_eq!(quote_identifier("idx_media"), "\"idx_media\"");
        assert_eq!(quote_identifier("idx\"media"), "\"idx\"\"media\"");
    }

    #[test]
    fn extracts_valid_fillfactor() {
        let options = vec!["autovacuum_vacuum_scale_factor=0.02".to_string()];
        assert_eq!(extract_table_fillfactor(&options), None);

        let options = vec!["fillfactor=85".to_string()];
        assert_eq!(extract_table_fillfactor(&options), Some(85));

        let options = vec!["fillfactor=0".to_string()];
        assert_eq!(extract_table_fillfactor(&options), None);
    }
}
