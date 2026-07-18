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

const SOFT_DELETE_RETENTION_DAYS: i32 = 30;
const MAX_ROOT_ROWS_PER_TABLE: i64 = 100;

#[derive(Debug, thiserror::Error)]
pub enum SoftDeletePurgeError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Failed to serialize soft-delete purge stats: {0}")]
    StatsSerialization(serde_json::Error),
    #[error("{failed_count} soft-delete purge operation(s) failed")]
    PurgeFailures { failed_count: usize },
}

#[derive(Debug, Clone, Serialize)]
struct SoftDeletePurgeStats {
    status: String,
    retention_days: i32,
    max_root_rows_per_table: i64,
    purged_root_rows: u64,
    failed_count: usize,
    entities: Vec<SoftDeletePurgeRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct SoftDeletePurgeRecord {
    entity_type: String,
    purged_root_rows: u64,
    action: String,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum PurgeTarget {
    Playlist,
    Library,
    User,
}

impl PurgeTarget {
    const ALL: [Self; 3] = [Self::Playlist, Self::Library, Self::User];

    fn entity_type(self) -> &'static str {
        match self {
            Self::Playlist => "playlist",
            Self::Library => "library",
            Self::User => "user",
        }
    }

    async fn delete_batch(self, pool: &sqlx::PgPool) -> Result<u64, sqlx::Error> {
        let deleted_ids: Vec<Uuid> = match self {
            Self::Playlist => {
                sqlx::query_scalar(
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM playlists
                        WHERE deleted_at < now() - ($1::integer * INTERVAL '1 day')
                        ORDER BY deleted_at ASC
                        LIMIT $2
                        FOR UPDATE SKIP LOCKED
                    )
                    DELETE FROM playlists
                    WHERE id IN (SELECT id FROM candidates)
                    RETURNING id
                    "#,
                )
                .bind(SOFT_DELETE_RETENTION_DAYS)
                .bind(MAX_ROOT_ROWS_PER_TABLE)
                .fetch_all(pool)
                .await?
            }
            Self::Library => {
                sqlx::query_scalar(
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM libraries
                        WHERE deleted_at < now() - ($1::integer * INTERVAL '1 day')
                        ORDER BY deleted_at ASC
                        LIMIT $2
                        FOR UPDATE SKIP LOCKED
                    )
                    DELETE FROM libraries
                    WHERE id IN (SELECT id FROM candidates)
                    RETURNING id
                    "#,
                )
                .bind(SOFT_DELETE_RETENTION_DAYS)
                .bind(MAX_ROOT_ROWS_PER_TABLE)
                .fetch_all(pool)
                .await?
            }
            Self::User => {
                sqlx::query_scalar(
                    r#"
                    WITH candidates AS (
                        SELECT id
                        FROM users
                        WHERE deleted_at < now() - ($1::integer * INTERVAL '1 day')
                          AND role <> 'owner'
                        ORDER BY deleted_at ASC
                        LIMIT $2
                        FOR UPDATE SKIP LOCKED
                    )
                    DELETE FROM users
                    WHERE id IN (SELECT id FROM candidates)
                    RETURNING id
                    "#,
                )
                .bind(SOFT_DELETE_RETENTION_DAYS)
                .bind(MAX_ROOT_ROWS_PER_TABLE)
                .fetch_all(pool)
                .await?
            }
        };
        Ok(deleted_ids.len() as u64)
    }
}

pub async fn run_soft_delete_purge(
    state: &AppState,
    task_id: Uuid,
    _task_config: serde_json::Value,
) -> Result<(), SoftDeletePurgeError> {
    tracing::info!(task_id = %task_id, "Starting soft-delete purge");
    let mut entities = Vec::with_capacity(PurgeTarget::ALL.len());
    let mut purged_root_rows = 0u64;
    let mut failed_count = 0usize;

    for target in PurgeTarget::ALL {
        match target.delete_batch(&state.pool).await {
            Ok(deleted_root_rows) => {
                purged_root_rows += deleted_root_rows;
                if deleted_root_rows > 0 {
                    metrics::counter!(
                        "soft_delete_purge_deleted_total",
                        "entity_type" => target.entity_type()
                    )
                    .increment(deleted_root_rows);
                }
                entities.push(SoftDeletePurgeRecord {
                    entity_type: target.entity_type().to_string(),
                    purged_root_rows: deleted_root_rows,
                    action: "purged".to_string(),
                    error: None,
                });
            }
            Err(error) => {
                failed_count += 1;
                let error = error.to_string();
                metrics::counter!(
                    "soft_delete_purge_failures_total",
                    "entity_type" => target.entity_type()
                )
                .increment(1);
                tracing::error!(
                    task_id = %task_id,
                    entity_type = target.entity_type(),
                    error = %error,
                    "Failed to purge expired soft-deleted records"
                );
                entities.push(SoftDeletePurgeRecord {
                    entity_type: target.entity_type().to_string(),
                    purged_root_rows: 0,
                    action: "failed".to_string(),
                    error: Some(error),
                });
            }
        }
    }

    let stats = SoftDeletePurgeStats {
        status: if failed_count == 0 {
            "completed".to_string()
        } else {
            "failed".to_string()
        },
        retention_days: SOFT_DELETE_RETENTION_DAYS,
        max_root_rows_per_table: MAX_ROOT_ROWS_PER_TABLE,
        purged_root_rows,
        failed_count,
        entities,
    };
    persist_run_stats(state, task_id, &stats).await?;
    if failed_count > 0 {
        return Err(SoftDeletePurgeError::PurgeFailures { failed_count });
    }

    tracing::info!(
        task_id = %task_id,
        purged_root_rows = stats.purged_root_rows,
        "Soft-delete purge completed"
    );
    Ok(())
}

async fn persist_run_stats(
    state: &AppState,
    task_id: Uuid,
    stats: &SoftDeletePurgeStats,
) -> Result<(), SoftDeletePurgeError> {
    let stats = serde_json::to_value(stats).map_err(SoftDeletePurgeError::StatsSerialization)?;
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

#[cfg(test)]
mod tests {
    use super::{PurgeTarget, SOFT_DELETE_RETENTION_DAYS};

    #[test]
    fn purge_targets_are_fixed_and_distinct() {
        let entity_types: Vec<_> = PurgeTarget::ALL
            .into_iter()
            .map(PurgeTarget::entity_type)
            .collect();
        assert_eq!(entity_types, ["playlist", "library", "user"]);
        assert_eq!(SOFT_DELETE_RETENTION_DAYS, 30);
    }
}
