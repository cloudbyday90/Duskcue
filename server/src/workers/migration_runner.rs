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

use serde_json::{Value, json};
use sqlx::{Postgres, Row, Transaction};
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

#[derive(Debug)]
struct MatchedImportRow {
    id: Uuid,
    platform_user_id: Option<Uuid>,
    matched_media_item_id: Option<Uuid>,
    source_is_watched: bool,
    source_play_count: i32,
    source_resume_position_ms: i64,
    source_last_played_at: Option<chrono::DateTime<chrono::Utc>>,
    import_batch_id: Uuid,
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

    let import_batch_id = Uuid::now_v7();
    let imported_count =
        import_matched_items(&state, migration_source_id, import_batch_id, &cancellation).await?;
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

    if imported_count > 0 {
        tracing::info!(
            migration_source_id = %migration_source_id,
            imported_count,
            "Migration runner imported matched watch-state rows"
        );
    }

    Ok(())
}

async fn import_matched_items(
    state: &AppState,
    migration_source_id: Uuid,
    import_batch_id: Uuid,
    cancellation: &CancellationToken,
) -> Result<u64, MigrationError> {
    let rows = sqlx::query(
        r#"
        SELECT
            l.id,
            m.platform_user_id,
            l.matched_media_item_id,
            l.source_is_watched,
            l.source_play_count,
            l.source_resume_position_ms,
            l.source_last_played_at
        FROM migration_import_log l
        JOIN migration_user_mapping m ON m.id = l.migration_user_mapping_id
        WHERE l.migration_source_id = $1
          AND l.status = 'matched'
        ORDER BY l.id
        "#,
    )
    .bind(migration_source_id)
    .fetch_all(&state.pool)
    .await?;

    let mut imported_count = 0_u64;
    for row in rows {
        if cancellation.is_cancelled() || is_source_cancelled(state, migration_source_id).await? {
            break;
        }

        let import_row = MatchedImportRow {
            id: row.get("id"),
            platform_user_id: row.get("platform_user_id"),
            matched_media_item_id: row.get("matched_media_item_id"),
            source_is_watched: row.get("source_is_watched"),
            source_play_count: row.get("source_play_count"),
            source_resume_position_ms: row.get("source_resume_position_ms"),
            source_last_played_at: row.get("source_last_played_at"),
            import_batch_id,
        };

        match import_single_matched_item(state, migration_source_id, &import_row).await {
            Ok(()) => imported_count += 1,
            Err(error) => {
                mark_import_log_error(
                    state,
                    migration_source_id,
                    import_row.id,
                    &error.to_string(),
                )
                .await?;
            }
        }
    }

    Ok(imported_count)
}

async fn import_single_matched_item(
    state: &AppState,
    migration_source_id: Uuid,
    row: &MatchedImportRow,
) -> Result<(), MigrationError> {
    let user_id = row.platform_user_id.ok_or_else(|| {
        MigrationError::InvalidSourceConfiguration(
            "matched import row is missing platform_user_id".to_string(),
        )
    })?;
    let media_item_id = row.matched_media_item_id.ok_or_else(|| {
        MigrationError::InvalidSourceConfiguration(
            "matched import row is missing matched_media_item_id".to_string(),
        )
    })?;
    let resume_position_ms =
        import_resume_position_ms(row.source_is_watched, row.source_resume_position_ms);
    let play_count = row.source_play_count.max(i32::from(row.source_is_watched));

    let mut tx = state.pool.begin().await?;
    let previous_user_item_data =
        load_previous_user_item_data(&mut tx, user_id, media_item_id).await?;
    let user_item_data_id: Uuid = sqlx::query(
        r#"
        INSERT INTO user_item_data (
            id,
            user_id,
            media_item_id,
            is_watched,
            play_count,
            last_played_at,
            resume_position_ms
        )
        VALUES (uuidv7(), $1, $2, $3, $4, $5, $6)
        ON CONFLICT (user_id, media_item_id) DO UPDATE SET
            is_watched = user_item_data.is_watched OR EXCLUDED.is_watched,
            play_count = GREATEST(user_item_data.play_count, EXCLUDED.play_count),
            last_played_at = CASE
                WHEN user_item_data.last_played_at IS NULL THEN EXCLUDED.last_played_at
                WHEN EXCLUDED.last_played_at IS NULL THEN user_item_data.last_played_at
                ELSE GREATEST(user_item_data.last_played_at, EXCLUDED.last_played_at)
            END,
            resume_position_ms = CASE
                WHEN user_item_data.is_watched OR EXCLUDED.is_watched THEN 0
                ELSE GREATEST(user_item_data.resume_position_ms, EXCLUDED.resume_position_ms)
            END,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .bind(row.source_is_watched)
    .bind(play_count)
    .bind(row.source_last_played_at)
    .bind(resume_position_ms)
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    sqlx::query(
        r#"
        UPDATE migration_import_log
        SET import_batch_id = $3,
            previous_user_item_data = $4,
            imported_user_item_data_id = $5,
            imported_at = now(),
            rolled_back_at = NULL,
            rollback_detail = NULL,
            status = 'imported',
            error_detail = NULL
        WHERE migration_source_id = $1
          AND id = $2
          AND status = 'matched'
        "#,
    )
    .bind(migration_source_id)
    .bind(row.id)
    .bind(row.import_batch_id)
    .bind(previous_user_item_data)
    .bind(user_item_data_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

async fn load_previous_user_item_data(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    media_item_id: Uuid,
) -> Result<Option<Value>, MigrationError> {
    let row = sqlx::query(
        r#"
        SELECT id, is_watched, play_count, last_played_at, resume_position_ms,
               last_played_media_file_id, is_favorite, user_rating,
               audio_stream_index, subtitle_stream_index, metadata
        FROM user_item_data
        WHERE user_id = $1
          AND media_item_id = $2
        FOR UPDATE
        "#,
    )
    .bind(user_id)
    .bind(media_item_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(row.map(|row| {
        json!({
            "id": row.get::<Uuid, _>("id"),
            "is_watched": row.get::<bool, _>("is_watched"),
            "play_count": row.get::<i32, _>("play_count"),
            "last_played_at": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_played_at"),
            "resume_position_ms": row.get::<i32, _>("resume_position_ms"),
            "last_played_media_file_id": row.get::<Option<Uuid>, _>("last_played_media_file_id"),
            "is_favorite": row.get::<bool, _>("is_favorite"),
            "user_rating": row.get::<Option<i32>, _>("user_rating"),
            "audio_stream_index": row.get::<Option<i32>, _>("audio_stream_index"),
            "subtitle_stream_index": row.get::<Option<i32>, _>("subtitle_stream_index"),
            "metadata": row.get::<Value, _>("metadata"),
        })
    }))
}

async fn mark_import_log_error(
    state: &AppState,
    migration_source_id: Uuid,
    import_log_id: Uuid,
    error: &str,
) -> Result<(), MigrationError> {
    sqlx::query(
        r#"
        UPDATE migration_import_log
        SET status = 'error',
            error_detail = $3
        WHERE migration_source_id = $1
          AND id = $2
        "#,
    )
    .bind(migration_source_id)
    .bind(import_log_id)
    .bind(error)
    .execute(&state.pool)
    .await?;

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
                COUNT(l.id) FILTER (WHERE l.status IN ('matched', 'imported', 'rolled_back', 'skipped'))::INT AS items_matched,
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
            COUNT(*) FILTER (WHERE status IN ('imported', 'rolled_back', 'skipped', 'unmatched', 'error'))::BIGINT AS terminal_items
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
    if summary.error_items > 0 {
        "failed"
    } else if summary.total_items == 0 || summary.matched_items > 0 {
        "pending"
    } else if summary.terminal_items == summary.total_items {
        "completed"
    } else {
        "pending"
    }
}

fn import_resume_position_ms(source_is_watched: bool, source_resume_position_ms: i64) -> i32 {
    if source_is_watched {
        0
    } else {
        source_resume_position_ms.clamp(0, i64::from(i32::MAX)) as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_resume_position_resets_watched_items_and_clamps_values() {
        assert_eq!(import_resume_position_ms(true, 120_000), 0);
        assert_eq!(import_resume_position_ms(false, -1), 0);
        assert_eq!(
            import_resume_position_ms(false, i64::from(i32::MAX) + 1),
            i32::MAX
        );
        assert_eq!(import_resume_position_ms(false, 90_000), 90_000);
    }

    #[test]
    fn final_status_prioritizes_errors_then_pending_matched_rows() {
        assert_eq!(
            final_status_from_summary(&MigrationRunSummary {
                total_items: 1,
                matched_items: 0,
                error_items: 1,
                terminal_items: 1,
            }),
            "failed"
        );
        assert_eq!(
            final_status_from_summary(&MigrationRunSummary {
                total_items: 1,
                matched_items: 1,
                error_items: 0,
                terminal_items: 0,
            }),
            "pending"
        );
        assert_eq!(
            final_status_from_summary(&MigrationRunSummary {
                total_items: 2,
                matched_items: 0,
                error_items: 0,
                terminal_items: 2,
            }),
            "completed"
        );
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
