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
use uuid::Uuid;

use crate::state::AppState;

use super::error::MigrationError;
use super::types::*;

const ACTIVE_STATUSES: &[&str] = &["discovering", "matching", "importing"];

pub fn validate_platform(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_PLATFORMS.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid platform: {value}"
        )))
    }
}

pub fn validate_status(value: &str) -> Result<(), MigrationError> {
    if VALID_MIGRATION_STATUSES.contains(&value) {
        Ok(())
    } else {
        Err(MigrationError::InvalidSourceConfiguration(format!(
            "invalid status: {value}"
        )))
    }
}

pub async fn create_migration_source(
    state: &AppState,
    request: CreateMigrationSourceRequest,
) -> Result<MigrationSourceResponse, MigrationError> {
    validate_platform(&request.platform)?;

    let row = sqlx::query(
        r#"
        INSERT INTO migration_sources (platform, name, connection_config)
        VALUES ($1, $2, $3)
        RETURNING id, created_at, platform, name, connection_config, last_run_at, status
        "#,
    )
    .bind(request.platform)
    .bind(request.name)
    .bind(request.connection_config)
    .fetch_one(&state.pool)
    .await?;

    Ok(row_to_source_response(&row))
}

pub async fn list_migration_sources(
    state: &AppState,
    query: ListMigrationSourcesQuery,
    page: u32,
    page_size: u32,
) -> Result<MigrationSourceListResponse, MigrationError> {
    if let Some(platform) = query.platform.as_deref() {
        validate_platform(platform)?;
    }
    if let Some(status) = query.status.as_deref() {
        validate_status(status)?;
    }

    let mut builder = sqlx::QueryBuilder::new(
        "SELECT id, created_at, platform, name, connection_config, last_run_at, status FROM migration_sources",
    );
    push_source_filters(&mut builder, &query);
    builder.push(" ORDER BY created_at DESC");
    let limit = page_size.max(1) as i64;
    let offset = (page.saturating_sub(1) as i64) * limit;
    builder.push(" LIMIT ").push_bind(limit);
    builder.push(" OFFSET ").push_bind(offset);

    let rows = builder.build().fetch_all(&state.pool).await?;
    let items = rows.iter().map(row_to_source_response).collect();

    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM migration_sources");
    push_source_filters(&mut count_builder, &query);
    let total: i64 = count_builder.build().fetch_one(&state.pool).await?.get(0);

    Ok(MigrationSourceListResponse {
        items,
        total,
        page,
        page_size,
        total_pages: ((total as f64) / (page_size as f64)).ceil() as u32,
    })
}

pub async fn get_migration_source(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationSourceResponse, MigrationError> {
    get_source(state, id).await
}

pub async fn delete_migration_source(state: &AppState, id: Uuid) -> Result<(), MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    let deleted = sqlx::query("DELETE FROM migration_sources WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?
        .rows_affected();

    if deleted == 0 {
        return Err(MigrationError::NotFound(id));
    }

    Ok(())
}

pub async fn test_connection(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration source is registered; source-specific connection checks are handled by later Phase 14 adapters".to_string(),
    })
}

pub async fn discover_source(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration source is ready for discovery; source-specific discovery is implemented in later Phase 14 tasks".to_string(),
    })
}

pub async fn save_user_mappings(
    state: &AppState,
    id: Uuid,
    request: SaveUserMappingsRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;

    if request.mappings.is_empty() {
        return Err(MigrationError::NoUserMappings);
    }

    validate_mapping_conflicts(&request)?;
    validate_platform_users_exist(state, &request).await?;

    let mut tx = state.pool.begin().await?;

    sqlx::query("DELETE FROM migration_user_mapping WHERE migration_source_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    for mapping in request.mappings {
        sqlx::query(
            r#"
            INSERT INTO migration_user_mapping (
                migration_source_id,
                source_user_id,
                source_user_name,
                platform_user_id
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(id)
        .bind(mapping.source_user_id)
        .bind(mapping.source_user_name)
        .bind(mapping.platform_user_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration user mappings saved".to_string(),
    })
}

pub async fn start_migration(
    state: &AppState,
    id: Uuid,
    request: StartMigrationRequest,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;
    ensure_not_active(id, &source.status)?;
    ensure_has_mappings(state, id).await?;

    let message = if request.dry_run.unwrap_or(false) {
        "Migration dry-run request accepted; preflight and runner behavior are implemented in later Phase 14 tasks"
    } else {
        "Migration start request accepted; runner behavior is implemented in later Phase 14 tasks"
    };

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: message.to_string(),
    })
}

pub async fn get_migration_progress(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationProgressResponse, MigrationError> {
    let source = get_source(state, id).await?;

    let row = sqlx::query(
        r#"
        SELECT
            COUNT(*)::INT AS items_discovered,
            COUNT(*) FILTER (WHERE status IN ('matched', 'imported', 'skipped'))::INT AS items_matched,
            COUNT(*) FILTER (WHERE status = 'unmatched')::INT AS items_unmatched,
            COUNT(*) FILTER (WHERE status = 'imported')::INT AS items_imported,
            COUNT(*) FILTER (WHERE status = 'skipped')::INT AS items_skipped,
            COUNT(*) FILTER (WHERE status IN ('imported', 'skipped', 'unmatched', 'error'))::INT AS items_processed
        FROM migration_import_log
        WHERE migration_source_id = $1
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    let items_discovered: i32 = row.get("items_discovered");
    let items_processed: i32 = row.get("items_processed");
    let percent_complete = if source.status == "completed" {
        100.0
    } else if items_discovered > 0 {
        ((items_processed as f32) / (items_discovered as f32) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };

    Ok(MigrationProgressResponse {
        migration_source_id: id,
        status: source.status,
        percent_complete,
        items_discovered,
        items_matched: row.get("items_matched"),
        items_unmatched: row.get("items_unmatched"),
        items_imported: row.get("items_imported"),
        items_skipped: row.get("items_skipped"),
    })
}

pub async fn get_unmatched_report(
    state: &AppState,
    id: Uuid,
    _query: UnmatchedReportQuery,
    page: u32,
    page_size: u32,
) -> Result<UnmatchedReportResponse, MigrationError> {
    get_source(state, id).await?;

    let limit = page_size.max(1) as i64;
    let offset = (page.saturating_sub(1) as i64) * limit;

    let rows = sqlx::query(
        r#"
        SELECT id, source_item_id, source_item_title, source_item_type,
               source_item_year, source_provider_ids, match_method, status, error_detail
        FROM migration_import_log
        WHERE migration_source_id = $1
          AND (status = 'unmatched' OR match_method = 'unmatched')
        ORDER BY source_item_title, source_item_year NULLS LAST, source_item_id
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    let total: i64 = sqlx::query(
        r#"
        SELECT COUNT(*)
        FROM migration_import_log
        WHERE migration_source_id = $1
          AND (status = 'unmatched' OR match_method = 'unmatched')
        "#,
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?
    .get(0);

    Ok(UnmatchedReportResponse {
        items: rows.iter().map(row_to_unmatched_item).collect(),
        total,
        page,
        page_size,
        total_pages: ((total as f64) / (page_size as f64)).ceil() as u32,
    })
}

pub async fn cancel_migration(
    state: &AppState,
    id: Uuid,
) -> Result<MigrationActionResponse, MigrationError> {
    let source = get_source(state, id).await?;

    if ACTIVE_STATUSES.contains(&source.status.as_str()) {
        let status = update_source_status(state, id, "cancelled").await?;
        return Ok(MigrationActionResponse {
            migration_source_id: id,
            status,
            message: "Migration cancellation recorded".to_string(),
        });
    }

    Ok(MigrationActionResponse {
        migration_source_id: id,
        status: source.status,
        message: "Migration is not running; no cancellation was needed".to_string(),
    })
}

async fn get_source(state: &AppState, id: Uuid) -> Result<MigrationSourceResponse, MigrationError> {
    let row = sqlx::query(
        "SELECT id, created_at, platform, name, connection_config, last_run_at, status FROM migration_sources WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(MigrationError::NotFound(id))?;

    Ok(row_to_source_response(&row))
}

async fn update_source_status(
    state: &AppState,
    id: Uuid,
    status: &str,
) -> Result<String, MigrationError> {
    let row = sqlx::query(
        r#"
        UPDATE migration_sources
        SET status = $2, last_run_at = COALESCE(last_run_at, now())
        WHERE id = $1
        RETURNING status
        "#,
    )
    .bind(id)
    .bind(status)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(MigrationError::NotFound(id))?;

    Ok(row.get("status"))
}

fn ensure_not_active(id: Uuid, status: &str) -> Result<(), MigrationError> {
    if ACTIVE_STATUSES.contains(&status) {
        Err(MigrationError::AlreadyInProgress(id))
    } else {
        Ok(())
    }
}

fn push_source_filters(
    builder: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    query: &ListMigrationSourcesQuery,
) {
    let mut where_started = false;

    if let Some(platform) = query.platform.as_deref() {
        builder.push(" WHERE platform = ").push_bind(platform);
        where_started = true;
    }

    if let Some(status) = query.status.as_deref() {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder.push(" status = ").push_bind(status);
    }
}

fn validate_mapping_conflicts(request: &SaveUserMappingsRequest) -> Result<(), MigrationError> {
    let mut source_user_ids = std::collections::HashSet::new();
    let mut platform_user_ids = std::collections::HashSet::new();

    for mapping in &request.mappings {
        if !source_user_ids.insert(mapping.source_user_id.as_str()) {
            return Err(MigrationError::UserMappingConflict(format!(
                "source user {} is mapped more than once",
                mapping.source_user_id
            )));
        }

        if !platform_user_ids.insert(mapping.platform_user_id) {
            return Err(MigrationError::UserMappingConflict(format!(
                "platform user {} is mapped more than once",
                mapping.platform_user_id
            )));
        }
    }

    Ok(())
}

async fn validate_platform_users_exist(
    state: &AppState,
    request: &SaveUserMappingsRequest,
) -> Result<(), MigrationError> {
    let platform_user_ids: Vec<Uuid> = request
        .mappings
        .iter()
        .map(|m| m.platform_user_id)
        .collect();

    let existing_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM users WHERE id = ANY($1) AND deleted_at IS NULL")
            .bind(&platform_user_ids)
            .fetch_one(&state.pool)
            .await?
            .get(0);

    if existing_count != platform_user_ids.len() as i64 {
        return Err(MigrationError::UserMappingConflict(
            "one or more platform users do not exist".to_string(),
        ));
    }

    Ok(())
}

async fn ensure_has_mappings(state: &AppState, id: Uuid) -> Result<(), MigrationError> {
    let count: i64 =
        sqlx::query("SELECT COUNT(*) FROM migration_user_mapping WHERE migration_source_id = $1")
            .bind(id)
            .fetch_one(&state.pool)
            .await?
            .get(0);

    if count == 0 {
        return Err(MigrationError::NoUserMappings);
    }

    Ok(())
}

fn row_to_source_response(row: &sqlx::postgres::PgRow) -> MigrationSourceResponse {
    MigrationSourceResponse {
        id: row.get("id"),
        created_at: row.get("created_at"),
        platform: row.get("platform"),
        name: row.get("name"),
        connection_config: row.get("connection_config"),
        last_run_at: row.get("last_run_at"),
        status: row.get("status"),
    }
}

fn row_to_unmatched_item(row: &sqlx::postgres::PgRow) -> UnmatchedItemResponse {
    UnmatchedItemResponse {
        id: row.get("id"),
        source_item_id: row.get("source_item_id"),
        source_item_title: row.get("source_item_title"),
        source_item_type: row.get("source_item_type"),
        source_item_year: row.get("source_item_year"),
        source_provider_ids: row.get("source_provider_ids"),
        match_method: row.get("match_method"),
        status: row.get("status"),
        error_detail: row.get("error_detail"),
    }
}
