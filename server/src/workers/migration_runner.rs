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
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domains::migration::MigrationError;
use crate::state::AppState;

const RUNNING_STATUS: &str = "importing";

#[derive(Debug)]
struct MigrationRunSummary {
    total_items: i64,
    matched_items: i64,
    error_items: i64,
    terminal_items: i64,
}

pub async fn spawn_migration_runner(
    state: &AppState,
    migration_source_id: Uuid,
) -> Result<String, MigrationError> {
    if state.migration_runs.contains_key(&migration_source_id) {
        return Err(MigrationError::AlreadyInProgress(migration_source_id));
    }

    let row = sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = $2, last_run_at = now()
        WHERE id = $1
          AND status NOT IN ('discovering', 'matching', 'importing')
        RETURNING status
        "#,
    )
    .bind(migration_source_id)
    .bind(RUNNING_STATUS)
    .fetch_optional(&state.pool)
    .await?;

    let Some(row) = row else {
        let exists: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM migration_sources WHERE id = $1")
                .bind(migration_source_id)
                .fetch_optional(&state.pool)
                .await?;
        return if exists.is_some() {
            Err(MigrationError::AlreadyInProgress(migration_source_id))
        } else {
            Err(MigrationError::NotFound(migration_source_id))
        };
    };

    let cancellation = CancellationToken::new();
    state
        .migration_runs
        .insert(migration_source_id, cancellation.clone());
    let task_state = state.clone();

    tokio::spawn(async move {
        let result = run_migration(task_state.clone(), migration_source_id, cancellation).await;
        task_state.migration_runs.remove(&migration_source_id);

        if let Err(error) = result {
            tracing::error!(
                migration_source_id = %migration_source_id,
                error = %error,
                "Migration runner failed"
            );
            if let Err(update_error) =
                mark_source_failed(&task_state, migration_source_id, &error.to_string()).await
            {
                tracing::error!(
                    migration_source_id = %migration_source_id,
                    error = %update_error,
                    "Failed to persist migration runner failure"
                );
            }
        }
    });

    Ok(row.get("status"))
}

pub fn cancel_migration_runner(state: &AppState, migration_source_id: Uuid) -> bool {
    let Some(entry) = state.migration_runs.get(&migration_source_id) else {
        return false;
    };
    entry.cancel();
    true
}

async fn run_migration(
    state: AppState,
    migration_source_id: Uuid,
    cancellation: CancellationToken,
) -> Result<(), MigrationError> {
    if cancellation.is_cancelled() || is_source_cancelled(&state, migration_source_id).await? {
        return Ok(());
    }

    recalculate_mapping_counters(&state, migration_source_id).await?;
    let summary = load_run_summary(&state, migration_source_id).await?;

    if cancellation.is_cancelled() || is_source_cancelled(&state, migration_source_id).await? {
        return Ok(());
    }

    let final_status = final_status_from_summary(&summary);
    sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = $2
        WHERE id = $1
          AND status = 'importing'
        "#,
    )
    .bind(migration_source_id)
    .bind(final_status)
    .execute(&state.pool)
    .await?;

    if summary.matched_items > 0 {
        tracing::info!(
            migration_source_id = %migration_source_id,
            matched_items = summary.matched_items,
            "Migration runner preserved matched rows for the import task"
        );
    }

    Ok(())
}

async fn recalculate_mapping_counters(
    state: &AppState,
    migration_source_id: Uuid,
) -> Result<(), MigrationError> {
    sqlx::query(
        r#"
        WITH rollup AS (
            SELECT
                m.id,
                COUNT(l.id) FILTER (WHERE l.status IN ('matched', 'imported', 'skipped'))::INT AS items_matched,
                COUNT(l.id) FILTER (WHERE l.status = 'unmatched')::INT AS items_unmatched,
                COUNT(l.id) FILTER (WHERE l.status = 'imported')::INT AS items_imported,
                COUNT(l.id) FILTER (WHERE l.status = 'skipped')::INT AS items_skipped,
                COUNT(l.id) FILTER (WHERE l.status = 'error')::INT AS items_error
            FROM migration_user_mapping m
            LEFT JOIN migration_import_log l ON l.migration_user_mapping_id = m.id
            WHERE m.migration_source_id = $1
            GROUP BY m.id
        )
        UPDATE migration_user_mapping m
        SET
            items_matched = rollup.items_matched,
            items_unmatched = rollup.items_unmatched,
            items_imported = rollup.items_imported,
            items_skipped = rollup.items_skipped,
            status = CASE
                WHEN rollup.items_error > 0 THEN 'failed'
                WHEN rollup.items_matched > 0
                  AND rollup.items_matched = rollup.items_imported + rollup.items_skipped THEN 'imported'
                ELSE m.status
            END,
            imported_at = CASE
                WHEN rollup.items_matched > 0
                  AND rollup.items_matched = rollup.items_imported + rollup.items_skipped THEN COALESCE(m.imported_at, now())
                ELSE m.imported_at
            END
        FROM rollup
        WHERE m.id = rollup.id
        "#,
    )
    .bind(migration_source_id)
    .execute(&state.pool)
    .await?;

    Ok(())
}

async fn load_run_summary(
    state: &AppState,
    migration_source_id: Uuid,
) -> Result<MigrationRunSummary, MigrationError> {
    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::BIGINT AS total_items,
            COUNT(*) FILTER (WHERE status = 'matched')::BIGINT AS matched_items,
            COUNT(*) FILTER (WHERE status = 'error')::BIGINT AS error_items,
            COUNT(*) FILTER (WHERE status IN ('imported', 'skipped', 'unmatched', 'error'))::BIGINT AS terminal_items
        FROM migration_import_log
        WHERE migration_source_id = $1
        "#,
    )
    .bind(migration_source_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(MigrationRunSummary {
        total_items: row.get("total_items"),
        matched_items: row.get("matched_items"),
        error_items: row.get("error_items"),
        terminal_items: row.get("terminal_items"),
    })
}

fn final_status_from_summary(summary: &MigrationRunSummary) -> &'static str {
    if summary.total_items == 0 || summary.matched_items > 0 {
        "pending"
    } else if summary.error_items > 0 {
        "failed"
    } else if summary.terminal_items == summary.total_items {
        "completed"
    } else {
        "pending"
    }
}

async fn is_source_cancelled(
    state: &AppState,
    migration_source_id: Uuid,
) -> Result<bool, MigrationError> {
    let status: Option<String> =
        sqlx::query_scalar("SELECT status FROM migration_sources WHERE id = $1")
            .bind(migration_source_id)
            .fetch_optional(&state.pool)
            .await?;

    Ok(status.as_deref() == Some("cancelled"))
}

async fn mark_source_failed(
    state: &AppState,
    migration_source_id: Uuid,
    error: &str,
) -> Result<(), MigrationError> {
    sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = 'failed'
        WHERE id = $1
          AND status = 'importing'
        "#,
    )
    .bind(migration_source_id)
    .execute(&state.pool)
    .await?;

    tracing::warn!(
        migration_source_id = %migration_source_id,
        error = %error,
        "Migration runner marked source failed"
    );

    Ok(())
}
