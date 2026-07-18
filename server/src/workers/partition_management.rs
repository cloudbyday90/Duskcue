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

use chrono::{Datelike, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

const PARTITIONED_TABLES: &[&str] = &["play_sessions", "play_events", "audit_log"];

#[derive(Debug, thiserror::Error)]
pub enum PartitionManagementError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to serialize partition-management stats: {0}")]
    StatsSerialization(serde_json::Error),
    #[error("{failed_count} partition creation operation(s) failed")]
    CreationFailures { failed_count: usize },
}

#[derive(Debug, Clone, Serialize)]
struct PartitionManagementStats {
    status: String,
    create_ahead_months: u32,
    partitions_created: usize,
    partitions_already_present: usize,
    failed_count: usize,
    partitions: Vec<PartitionRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct PartitionRecord {
    parent_table: String,
    partition_name: String,
    start_at: String,
    end_at: String,
    action: String,
    error: Option<String>,
}

pub async fn run_partition_management(
    state: &AppState,
    task_id: Uuid,
    task_config: serde_json::Value,
) -> Result<(), PartitionManagementError> {
    let create_ahead_months = task_config
        .get("create_ahead_months")
        .and_then(|value| value.as_u64())
        .unwrap_or(2)
        .clamp(1, 12) as u32;
    let current = Month::current();
    let mut partitions = Vec::new();
    let mut failed_count = 0usize;
    let mut partitions_created = 0usize;
    let mut partitions_already_present = 0usize;

    for offset in 0..=create_ahead_months {
        let month = current.add(offset);
        for parent_table in PARTITIONED_TABLES {
            match ensure_partition(&state.pool, parent_table, month).await {
                Ok(PartitionAction::Created(record)) => {
                    partitions_created += 1;
                    partitions.push(record);
                }
                Ok(PartitionAction::AlreadyPresent(record)) => {
                    partitions_already_present += 1;
                    partitions.push(record);
                }
                Err(error) => {
                    failed_count += 1;
                    let partition_name = partition_name(parent_table, month);
                    tracing::error!(
                        task_id = %task_id,
                        parent_table,
                        partition_name,
                        error = %error,
                        "Failed to create scheduled table partition"
                    );
                    partitions.push(PartitionRecord {
                        parent_table: (*parent_table).to_string(),
                        partition_name,
                        start_at: month.start_date(),
                        end_at: month.add(1).start_date(),
                        action: "failed".to_string(),
                        error: Some(error.to_string()),
                    });
                }
            }
        }
    }

    let stats = PartitionManagementStats {
        status: if failed_count == 0 {
            "completed".to_string()
        } else {
            "failed".to_string()
        },
        create_ahead_months,
        partitions_created,
        partitions_already_present,
        failed_count,
        partitions,
    };
    persist_run_stats(state, task_id, &stats).await?;
    if failed_count > 0 {
        return Err(PartitionManagementError::CreationFailures { failed_count });
    }
    tracing::info!(
        task_id = %task_id,
        created = stats.partitions_created,
        already_present = stats.partitions_already_present,
        "Partition management completed"
    );
    Ok(())
}

enum PartitionAction {
    Created(PartitionRecord),
    AlreadyPresent(PartitionRecord),
}

async fn ensure_partition(
    pool: &sqlx::PgPool,
    parent_table: &str,
    month: Month,
) -> Result<PartitionAction, sqlx::Error> {
    let partition_name = partition_name(parent_table, month);
    let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
        .bind(format!("public.{partition_name}"))
        .fetch_one(pool)
        .await?;
    let record = PartitionRecord {
        parent_table: parent_table.to_string(),
        partition_name: partition_name.clone(),
        start_at: month.start_date(),
        end_at: month.add(1).start_date(),
        action: String::new(),
        error: None,
    };
    if exists {
        return Ok(PartitionAction::AlreadyPresent(PartitionRecord {
            action: "already_present".to_string(),
            ..record
        }));
    }
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {partition_name} PARTITION OF {parent_table} FOR VALUES FROM ('{}') TO ('{}')",
        month.start_date(),
        month.add(1).start_date(),
    );
    sqlx::query(sqlx::AssertSqlSafe(sql)).execute(pool).await?;
    Ok(PartitionAction::Created(PartitionRecord {
        action: "created".to_string(),
        ..record
    }))
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &PartitionManagementStats,
) -> Result<(), PartitionManagementError> {
    let stats =
        serde_json::to_value(stats).map_err(PartitionManagementError::StatsSerialization)?;
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

#[derive(Clone, Copy)]
struct Month {
    year: i32,
    month: u32,
}

impl Month {
    fn current() -> Self {
        let now = Utc::now();
        Self {
            year: now.year(),
            month: now.month(),
        }
    }

    fn add(self, offset: u32) -> Self {
        let absolute = self.year * 12 + self.month as i32 - 1 + offset as i32;
        Self {
            year: absolute / 12,
            month: (absolute % 12 + 1) as u32,
        }
    }

    fn start_date(self) -> String {
        format!("{:04}-{:02}-01", self.year, self.month)
    }
}

fn partition_name(parent_table: &str, month: Month) -> String {
    format!("{parent_table}_{:04}_{:02}", month.year, month.month)
}

#[cfg(test)]
mod tests {
    use super::{Month, partition_name};

    #[test]
    fn month_add_crosses_year_boundaries() {
        let december = Month {
            year: 2026,
            month: 12,
        };
        assert_eq!(december.add(1).start_date(), "2027-01-01");
        assert_eq!(december.add(14).start_date(), "2028-02-01");
    }

    #[test]
    fn partition_name_is_deterministic() {
        assert_eq!(
            partition_name(
                "play_sessions",
                Month {
                    year: 2026,
                    month: 7
                }
            ),
            "play_sessions_2026_07"
        );
    }
}
