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

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::error::AppError;
use crate::services::collections as collection_builders;
use crate::services::conditions;
use crate::services::tmdb_client::TmdbClient;
use crate::state::AppState;

use super::error::CollectionsError;
use super::types::*;

pub fn validate_collection_type(value: &str) -> Result<(), CollectionsError> {
    if VALID_COLLECTION_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid collection_type: {value}"
        )))
    }
}

pub fn validate_visibility(value: &str) -> Result<(), CollectionsError> {
    if VALID_VISIBILITY.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid visibility: {value}"
        )))
    }
}

pub fn validate_sync_mode(value: &str) -> Result<(), CollectionsError> {
    if VALID_SYNC_MODES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid sync_mode: {value}"
        )))
    }
}

pub fn validate_template_type(value: &str) -> Result<(), CollectionsError> {
    if VALID_TEMPLATE_TYPES.contains(&value) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid template_type: {value}"
        )))
    }
}

pub fn validate_dynamic_config(config: &serde_json::Value) -> Result<(), CollectionsError> {
    let builder_type = config
        .get("builder_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            CollectionsError::InvalidDynamicConfig("dynamic_config.builder_type is required".into())
        })?;

    if VALID_BUILDER_TYPES.contains(&builder_type) {
        Ok(())
    } else {
        Err(CollectionsError::InvalidDynamicConfig(format!(
            "invalid builder_type: {builder_type}"
        )))
    }
}

pub fn validate_smart_filter(filter: &serde_json::Value) -> Result<(), CollectionsError> {
    conditions::validate_structure(filter)
        .map_err(|e| CollectionsError::InvalidSmartFilter(e.to_string()))
}

pub fn generate_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub async fn list_collections(
    _pool: &PgPool,
    query: ListCollectionsQuery,
    page: u32,
    page_size: u32,
) -> Result<CollectionListResponse, CollectionsError> {
    let mut builder = sqlx::QueryBuilder::new(SELECT_CLAUSE);
    let mut where_started = false;
    if let Some(lib) = query.library_id {
        builder.push(" WHERE library_id = ").push_bind(lib);
        where_started = true;
    }
    if let Some(ref collection_type) = query.collection_type {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder
            .push(" collection_type = ")
            .push_bind(collection_type);
        where_started = true;
    }
    if let Some(ref visibility) = query.visibility {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder.push(" visibility = ").push_bind(visibility);
        where_started = true;
    }
    if let Some(enabled) = query.enabled {
        builder.push(if where_started { " AND" } else { " WHERE" });
        builder.push(" is_enabled = ").push_bind(enabled);
    }

    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM collections");
    let mut count_where = false;
    if let Some(lib) = query.library_id {
        count_builder.push(" WHERE library_id = ").push_bind(lib);
        count_where = true;
    }
    if let Some(ref collection_type) = query.collection_type {
        count_builder.push(if count_where { " AND" } else { " WHERE" });
        count_builder
            .push(" collection_type = ")
            .push_bind(collection_type);
        count_where = true;
    }
    if let Some(ref visibility) = query.visibility {
        count_builder.push(if count_where { " AND" } else { " WHERE" });
        count_builder.push(" visibility = ").push_bind(visibility);
        count_where = true;
    }
    if let Some(enabled) = query.enabled {
        count_builder.push(if count_where { " AND" } else { " WHERE" });
        count_builder.push(" is_enabled = ").push_bind(enabled);
    }

    builder.push(" ORDER BY sort_order, name");
    let limit: i64 = page_size.max(1) as i64;
    let offset: i64 = (page.saturating_sub(1) as i64) * limit;
    builder.push(" LIMIT ").push_bind(limit);
    builder.push(" OFFSET ").push_bind(offset);

    let rows = builder.build().fetch_all(_pool).await?;
    let items: Vec<CollectionResponse> = rows
        .iter()
        .map(row_to_collection_row)
        .map(row_to_response)
        .collect();

    let total: i64 = count_builder
        .build()
        .fetch_one(_pool)
        .await?
        .try_get("count")
        .unwrap_or(0);

    Ok(CollectionListResponse {
        items,
        total,
        page,
        page_size,
    })
}

pub async fn get_collection(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<CollectionResponse, CollectionsError> {
    let mut builder = sqlx::QueryBuilder::new(SELECT_CLAUSE);
    builder.push(" WHERE id = ").push_bind(collection_id);
    let row = builder.build().fetch_optional(pool).await?;
    match row {
        Some(row) => Ok(row_to_response(row_to_collection_row(&row))),
        None => Err(CollectionsError::NotFound),
    }
}

pub async fn create_collection(
    pool: &PgPool,
    req: CreateCollectionRequest,
) -> Result<CollectionResponse, CollectionsError> {
    let slug = generate_slug(&req.name);
    let collection_type = req.collection_type.unwrap_or_else(|| "static".to_string());
    let visibility = req.visibility.unwrap_or_else(|| "visible".to_string());
    let sync_mode = req.sync_mode.unwrap_or_else(|| "sync".to_string());
    let schedule = req.schedule.unwrap_or_else(|| "0 6 * * *".to_string());
    let sort_by = req.sort_by.unwrap_or_else(|| "title.asc".to_string());
    let is_dynamic = collection_type == "dynamic";
    let is_smart = collection_type == "smart";
    let metadata = req.metadata.unwrap_or(serde_json::json!({}));

    check_name_unique(pool, req.library_id, &req.name).await?;

    let row = sqlx::query(
        r#"INSERT INTO collections
           (library_id, name, slug, description, collection_type, visibility,
            is_dynamic, dynamic_config, is_smart, smart_filter,
            poster_artwork_id, backdrop_artwork_id, sort_order, sort_by,
            sync_mode, schedule, is_enabled, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18)
           RETURNING id, created_at, updated_at, library_id, name, slug, description,
                     collection_type, visibility, is_dynamic, dynamic_config, is_smart, smart_filter,
                     poster_artwork_id, backdrop_artwork_id, sort_order, sort_by, item_count,
                     total_duration_seconds, sync_mode, schedule, last_synced_at, last_sync_result,
                     is_enabled, is_system, metadata"#,
    )
    .bind(req.library_id)
    .bind(req.name)
    .bind(slug)
    .bind(req.description)
    .bind(&collection_type)
    .bind(&visibility)
    .bind(is_dynamic)
    .bind(req.dynamic_config)
    .bind(is_smart)
    .bind(req.smart_filter)
    .bind(req.poster_artwork_id)
    .bind(req.backdrop_artwork_id)
    .bind(req.sort_order.unwrap_or(0))
    .bind(&sort_by)
    .bind(&sync_mode)
    .bind(&schedule)
    .bind(req.is_enabled.unwrap_or(true))
    .bind(metadata)
    .fetch_one(pool)
    .await?;

    Ok(row_to_response(row_to_collection_row(&row)))
}

pub async fn update_collection(
    pool: &PgPool,
    collection_id: Uuid,
    req: UpdateCollectionRequest,
) -> Result<CollectionResponse, CollectionsError> {
    let mut builder = sqlx::QueryBuilder::new("UPDATE collections SET updated_at = now()");
    if let Some(name) = &req.name {
        check_name_unique(pool, req.library_id, name).await?;
        builder.push(", name = ").push_bind(name.clone());
        builder.push(", slug = ").push_bind(generate_slug(name));
    }
    if let Some(library_id) = req.library_id {
        builder.push(", library_id = ").push_bind(library_id);
    }
    if let Some(description) = &req.description {
        builder
            .push(", description = ")
            .push_bind(description.clone());
    }
    if let Some(collection_type) = &req.collection_type {
        builder
            .push(", collection_type = ")
            .push_bind(collection_type.clone());
        let is_dynamic = collection_type == "dynamic";
        let is_smart = collection_type == "smart";
        builder.push(", is_dynamic = ").push_bind(is_dynamic);
        builder.push(", is_smart = ").push_bind(is_smart);
    }
    if let Some(visibility) = &req.visibility {
        builder
            .push(", visibility = ")
            .push_bind(visibility.clone());
    }
    if let Some(dynamic_config) = req.dynamic_config {
        builder
            .push(", dynamic_config = ")
            .push_bind(dynamic_config);
    }
    if let Some(smart_filter) = req.smart_filter {
        builder.push(", smart_filter = ").push_bind(smart_filter);
    }
    if let Some(poster_artwork_id) = req.poster_artwork_id {
        builder
            .push(", poster_artwork_id = ")
            .push_bind(poster_artwork_id);
    }
    if let Some(backdrop_artwork_id) = req.backdrop_artwork_id {
        builder
            .push(", backdrop_artwork_id = ")
            .push_bind(backdrop_artwork_id);
    }
    if let Some(sort_order) = req.sort_order {
        builder.push(", sort_order = ").push_bind(sort_order);
    }
    if let Some(sort_by) = &req.sort_by {
        builder.push(", sort_by = ").push_bind(sort_by.clone());
    }
    if let Some(sync_mode) = &req.sync_mode {
        builder.push(", sync_mode = ").push_bind(sync_mode.clone());
    }
    if let Some(schedule) = &req.schedule {
        builder.push(", schedule = ").push_bind(schedule.clone());
    }
    if let Some(is_enabled) = req.is_enabled {
        builder.push(", is_enabled = ").push_bind(is_enabled);
    }
    if let Some(metadata) = req.metadata {
        builder.push(", metadata = ").push_bind(metadata);
    }

    builder.push(" WHERE id = ").push_bind(collection_id);
    builder.push(" RETURNING ");
    builder.push(RETURNING_COLUMNS);

    let row = builder
        .build()
        .fetch_optional(pool)
        .await?
        .ok_or(CollectionsError::NotFound)?;
    Ok(row_to_response(row_to_collection_row(&row)))
}

pub async fn delete_collection(pool: &PgPool, collection_id: Uuid) -> Result<(), AppError> {
    let row = sqlx::query("SELECT is_system FROM collections WHERE id = $1")
        .bind(collection_id)
        .fetch_optional(pool)
        .await
        .map_err(CollectionsError::from)?;
    match row {
        None => Err(AppError::from(CollectionsError::NotFound)),
        Some(row) => {
            let is_system: bool = row
                .try_get("is_system")
                .map_err(|e| AppError::from(CollectionsError::Database(e)))?;
            if is_system {
                return Err(AppError::Conflict(
                    "system collections cannot be deleted; disable them instead".into(),
                ));
            }
            sqlx::query("DELETE FROM collections WHERE id = $1")
                .bind(collection_id)
                .execute(pool)
                .await
                .map_err(|e| AppError::from(CollectionsError::Database(e)))?;
            Ok(())
        }
    }
}

pub async fn list_collection_items(
    pool: &PgPool,
    collection_id: Uuid,
    query: ListCollectionItemsQuery,
    page: u32,
    page_size: u32,
) -> Result<CollectionItemsResponse, CollectionsError> {
    let include_missing = query.include_missing.unwrap_or(true);
    let limit: i64 = page_size.max(1) as i64;
    let offset: i64 = (page.saturating_sub(1) as i64) * limit;

    let rows = if include_missing {
        sqlx::query(
            r#"SELECT id, created_at, collection_id, media_item_id, position,
                      is_missing, missing_reason
               FROM collection_items
               WHERE collection_id = $1
               ORDER BY position, created_at
               LIMIT $2 OFFSET $3"#,
        )
        .bind(collection_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT id, created_at, collection_id, media_item_id, position,
                      is_missing, missing_reason
               FROM collection_items
               WHERE collection_id = $1 AND is_missing = false
               ORDER BY position, created_at
               LIMIT $2 OFFSET $3"#,
        )
        .bind(collection_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?
    };

    let items: Vec<CollectionItemResponse> = rows.iter().map(row_to_item_response).collect();

    let total: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM collection_items WHERE collection_id = $1"#)
            .bind(collection_id)
            .fetch_one(pool)
            .await?;

    Ok(CollectionItemsResponse {
        items,
        total,
        page,
        page_size,
    })
}

pub async fn add_collection_items(
    pool: &PgPool,
    collection_id: Uuid,
    req: AddCollectionItemsRequest,
) -> Result<CollectionItemsResponse, CollectionsError> {
    let starting_position = match req.starting_position {
        Some(pos) => pos,
        None => {
            let max_pos: Option<i32> = sqlx::query_scalar(
                r#"SELECT MAX(position) FROM collection_items WHERE collection_id = $1"#,
            )
            .bind(collection_id)
            .fetch_one(pool)
            .await?;
            max_pos.unwrap_or(0) + 1000
        }
    };

    let mut current_position = starting_position;
    for media_item_id in &req.media_item_ids {
        sqlx::query(
            r#"INSERT INTO collection_items (collection_id, media_item_id, position)
               VALUES ($1, $2, $3)
               ON CONFLICT (collection_id, media_item_id) DO NOTHING"#,
        )
        .bind(collection_id)
        .bind(media_item_id)
        .bind(current_position)
        .execute(pool)
        .await?;
        current_position += 1000;
    }

    update_collection_counters(pool, collection_id).await?;

    list_collection_items(
        pool,
        collection_id,
        ListCollectionItemsQuery {
            include_missing: Some(true),
            page: None,
            page_size: None,
        },
        1,
        req.media_item_ids.len().max(1) as u32,
    )
    .await
}

pub async fn reorder_collection_items(
    pool: &PgPool,
    collection_id: Uuid,
    req: ReorderCollectionItemsRequest,
) -> Result<CollectionItemsResponse, CollectionsError> {
    for item in &req.items {
        sqlx::query(
            r#"UPDATE collection_items SET position = $1
               WHERE collection_id = $2 AND media_item_id = $3"#,
        )
        .bind(item.position)
        .bind(collection_id)
        .bind(item.media_item_id)
        .execute(pool)
        .await?;
    }

    list_collection_items(
        pool,
        collection_id,
        ListCollectionItemsQuery {
            include_missing: Some(true),
            page: None,
            page_size: None,
        },
        1,
        req.items.len().max(1) as u32,
    )
    .await
}

pub async fn remove_collection_item(
    pool: &PgPool,
    collection_id: Uuid,
    media_item_id: Uuid,
) -> Result<(), CollectionsError> {
    let result = sqlx::query(
        r#"DELETE FROM collection_items WHERE collection_id = $1 AND media_item_id = $2"#,
    )
    .bind(collection_id)
    .bind(media_item_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(CollectionsError::NotFound);
    }

    update_collection_counters(pool, collection_id).await?;
    Ok(())
}

pub async fn sync_collections(
    state: &AppState,
    req: SyncCollectionsRequest,
) -> Result<SyncCollectionResponse, CollectionsError> {
    let metadata = state.runtime_config.load().metadata.clone();
    let tmdb_client =
        if metadata.providers.tmdb.enabled && !metadata.providers.tmdb.access_token.is_empty() {
            Some(TmdbClient::new(
                &metadata.providers.tmdb,
                metadata.metadata_language.clone(),
            ))
        } else {
            None
        };

    let include_external = req.include_external.unwrap_or(true);
    let reprocess_all = req.reprocess_all.unwrap_or(false);
    let results = collection_builders::sync_dynamic_collections(
        &state.pool,
        tmdb_client.as_ref(),
        req.library_id,
        include_external,
        reprocess_all,
    )
    .await
    .map_err(map_builder_error)?;

    Ok(SyncCollectionResponse {
        status: "synced".to_string(),
        queued_collections: results.len() as i64,
    })
}

pub async fn sync_collection(
    state: &AppState,
    collection_id: Uuid,
    req: SyncCollectionRequest,
) -> Result<SyncCollectionResponse, CollectionsError> {
    let metadata = state.runtime_config.load().metadata.clone();
    let tmdb_client =
        if metadata.providers.tmdb.enabled && !metadata.providers.tmdb.access_token.is_empty() {
            Some(TmdbClient::new(
                &metadata.providers.tmdb,
                metadata.metadata_language.clone(),
            ))
        } else {
            None
        };

    let include_external = req.include_external.unwrap_or(true);
    let reprocess_all = req.reprocess_all.unwrap_or(false);
    collection_builders::sync_dynamic_collection(
        &state.pool,
        tmdb_client.as_ref(),
        collection_id,
        include_external,
        reprocess_all,
    )
    .await
    .map_err(map_builder_error)?;

    Ok(SyncCollectionResponse {
        status: "synced".to_string(),
        queued_collections: 1,
    })
}

pub async fn list_templates(
    pool: &PgPool,
) -> Result<Vec<CollectionTemplateSummary>, CollectionsError> {
    let rows = sqlx::query(
        r#"SELECT id, name, description, template_type, author, source_url, is_system
           FROM collection_templates
           ORDER BY name"#,
    )
    .fetch_all(pool)
    .await?;

    let summaries: Vec<CollectionTemplateSummary> = rows
        .iter()
        .map(|row| CollectionTemplateSummary {
            id: row.try_get("id").unwrap_or_default(),
            name: row.try_get("name").unwrap_or_default(),
            description: row.try_get("description").ok().flatten(),
            template_type: row
                .try_get("template_type")
                .unwrap_or_else(|_| "single".into()),
            author: row.try_get("author").ok().flatten(),
            source_url: row.try_get("source_url").ok().flatten(),
            is_system: row.try_get("is_system").unwrap_or(false),
        })
        .collect();

    Ok(summaries)
}

pub async fn import_template(
    pool: &PgPool,
    req: ImportCollectionTemplateRequest,
) -> Result<CollectionTemplateResponse, CollectionsError> {
    let metadata = req.metadata.unwrap_or(serde_json::json!({}));
    let row = sqlx::query(
        r#"INSERT INTO collection_templates
           (name, description, template_type, template_json, author, source_url, metadata)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT (name) DO UPDATE SET
               description = COALESCE(EXCLUDED.description, collection_templates.description),
               template_type = EXCLUDED.template_type,
               template_json = EXCLUDED.template_json,
               author = COALESCE(EXCLUDED.author, collection_templates.author),
               source_url = COALESCE(EXCLUDED.source_url, collection_templates.source_url),
               metadata = EXCLUDED.metadata,
               updated_at = now()
           RETURNING id, name, description, template_type, template_json, author, source_url,
                     is_system, metadata, created_at, updated_at"#,
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.template_type)
    .bind(&req.template_json)
    .bind(&req.author)
    .bind(&req.source_url)
    .bind(metadata)
    .fetch_one(pool)
    .await?;

    let r = row_to_template_row(&row)?;
    Ok(template_row_to_response(r))
}

const SELECT_CLAUSE: &str = "SELECT id, created_at, updated_at, library_id, name, slug, description, collection_type, visibility, is_dynamic, dynamic_config, is_smart, smart_filter, poster_artwork_id, backdrop_artwork_id, sort_order, sort_by, item_count, total_duration_seconds, sync_mode, schedule, last_synced_at, last_sync_result, is_enabled, is_system, metadata FROM collections";

const RETURNING_COLUMNS: &str = "id, created_at, updated_at, library_id, name, slug, description, collection_type, visibility, is_dynamic, dynamic_config, is_smart, smart_filter, poster_artwork_id, backdrop_artwork_id, sort_order, sort_by, item_count, total_duration_seconds, sync_mode, schedule, last_synced_at, last_sync_result, is_enabled, is_system, metadata";

async fn check_name_unique(
    pool: &PgPool,
    library_id: Option<Uuid>,
    name: &str,
) -> Result<(), CollectionsError> {
    let existing: Option<i64> = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM collections
           WHERE name = $1
             AND (library_id IS NOT DISTINCT FROM $2)
             AND is_system = false"#,
    )
    .bind(name)
    .bind(library_id)
    .fetch_one(pool)
    .await?;
    if existing.unwrap_or(0) > 0 {
        return Err(CollectionsError::NameAlreadyExists);
    }
    Ok(())
}

async fn update_collection_counters(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<(), CollectionsError> {
    sqlx::query(
        r#"UPDATE collections
           SET item_count = (
               SELECT COUNT(*)::int FROM collection_items WHERE collection_id = $1
           )
           WHERE id = $1"#,
    )
    .bind(collection_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn row_to_collection_row(row: &sqlx::postgres::PgRow) -> CollectionRow {
    let metadata: serde_json::Value = row.try_get("metadata").unwrap_or(serde_json::Value::Null);
    let metadata = if metadata.is_null() {
        serde_json::json!({})
    } else {
        metadata
    };
    CollectionRow {
        id: row.try_get("id").unwrap_or_default(),
        created_at: row
            .try_get("created_at")
            .unwrap_or_else(|_| chrono::Utc::now()),
        updated_at: row
            .try_get("updated_at")
            .unwrap_or_else(|_| chrono::Utc::now()),
        library_id: row.try_get("library_id").ok().flatten(),
        name: row.try_get("name").unwrap_or_default(),
        slug: row.try_get("slug").unwrap_or_default(),
        description: row.try_get("description").ok().flatten(),
        collection_type: row
            .try_get("collection_type")
            .unwrap_or_else(|_| "static".into()),
        visibility: row
            .try_get("visibility")
            .unwrap_or_else(|_| "visible".into()),
        is_dynamic: row.try_get("is_dynamic").unwrap_or(false),
        dynamic_config: row.try_get("dynamic_config").ok().flatten(),
        is_smart: row.try_get("is_smart").unwrap_or(false),
        smart_filter: row.try_get("smart_filter").ok().flatten(),
        poster_artwork_id: row.try_get("poster_artwork_id").ok().flatten(),
        backdrop_artwork_id: row.try_get("backdrop_artwork_id").ok().flatten(),
        sort_order: row.try_get("sort_order").unwrap_or(0),
        sort_by: row
            .try_get("sort_by")
            .unwrap_or_else(|_| "title.asc".into()),
        item_count: row.try_get("item_count").unwrap_or(0),
        total_duration_seconds: row.try_get("total_duration_seconds").unwrap_or(0),
        sync_mode: row.try_get("sync_mode").unwrap_or_else(|_| "sync".into()),
        schedule: row
            .try_get("schedule")
            .unwrap_or_else(|_| "0 6 * * *".into()),
        last_synced_at: row.try_get("last_synced_at").ok().flatten(),
        last_sync_result: row.try_get("last_sync_result").ok().flatten(),
        is_enabled: row.try_get("is_enabled").unwrap_or(true),
        is_system: row.try_get("is_system").unwrap_or(false),
        metadata,
    }
}

fn row_to_response(row: CollectionRow) -> CollectionResponse {
    CollectionResponse {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        slug: row.slug,
        description: row.description,
        collection_type: row.collection_type,
        visibility: row.visibility,
        is_dynamic: row.is_dynamic,
        dynamic_config: row.dynamic_config,
        is_smart: row.is_smart,
        smart_filter: row.smart_filter,
        poster_artwork_id: row.poster_artwork_id,
        backdrop_artwork_id: row.backdrop_artwork_id,
        sort_order: row.sort_order,
        sort_by: row.sort_by,
        item_count: row.item_count,
        total_duration_seconds: row.total_duration_seconds,
        sync_mode: row.sync_mode,
        schedule: row.schedule,
        last_synced_at: row.last_synced_at,
        last_sync_result: row.last_sync_result,
        is_enabled: row.is_enabled,
        is_system: row.is_system,
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn row_to_item_response(row: &sqlx::postgres::PgRow) -> CollectionItemResponse {
    CollectionItemResponse {
        id: row.try_get("id").unwrap_or_default(),
        collection_id: row.try_get("collection_id").unwrap_or_default(),
        media_item_id: row.try_get("media_item_id").unwrap_or_default(),
        position: row.try_get("position").unwrap_or(0),
        is_missing: row.try_get("is_missing").unwrap_or(false),
        missing_reason: row.try_get("missing_reason").ok().flatten(),
        created_at: row
            .try_get("created_at")
            .unwrap_or_else(|_| chrono::Utc::now()),
    }
}

fn row_to_template_row(
    row: &sqlx::postgres::PgRow,
) -> Result<CollectionTemplateRow, CollectionsError> {
    Ok(CollectionTemplateRow {
        id: row.try_get("id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        name: row.try_get("name")?,
        description: row.try_get("description").ok().flatten(),
        template_type: row.try_get("template_type")?,
        template_json: row.try_get("template_json")?,
        author: row.try_get("author").ok().flatten(),
        source_url: row.try_get("source_url").ok().flatten(),
        is_system: row.try_get("is_system")?,
        metadata: row.try_get("metadata")?,
    })
}

fn template_row_to_response(row: CollectionTemplateRow) -> CollectionTemplateResponse {
    CollectionTemplateResponse {
        id: row.id,
        name: row.name,
        description: row.description,
        template_type: row.template_type,
        template_json: row.template_json,
        author: row.author,
        source_url: row.source_url,
        is_system: row.is_system,
        metadata: row.metadata,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn map_builder_error(err: collection_builders::CollectionBuilderError) -> CollectionsError {
    match err {
        collection_builders::CollectionBuilderError::InvalidConfig(msg) => {
            CollectionsError::InvalidDynamicConfig(msg)
        }
        collection_builders::CollectionBuilderError::ExternalUnavailable(source) => {
            CollectionsError::ExternalSourceUnavailable(source)
        }
        collection_builders::CollectionBuilderError::ExternalRateLimited => {
            CollectionsError::ExternalRateLimited
        }
        collection_builders::CollectionBuilderError::Database(e) => CollectionsError::Database(e),
    }
}
