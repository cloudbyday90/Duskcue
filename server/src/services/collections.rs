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

use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde::Serialize;
use sqlx::{PgPool, Row};
use thiserror::Error;
use uuid::Uuid;

use crate::services::metadata::MetadataError;
use crate::services::tmdb_client::{TmdbChart, TmdbChartItem, TmdbClient};

#[derive(Debug, Error)]
pub enum CollectionBuilderError {
    #[error("invalid collection builder config: {0}")]
    InvalidConfig(String),

    #[error("external collection builder source unavailable: {0}")]
    ExternalUnavailable(String),

    #[error("external collection builder source rate limited")]
    ExternalRateLimited,

    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct CollectionSyncResult {
    pub collection_id: Uuid,
    pub builder_type: String,
    pub added: usize,
    pub removed: usize,
    pub total_matched: usize,
    pub missing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionBuilderResult {
    pub key: String,
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<CollectionBuilderItem>,
    pub missing_external_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionBuilderItem {
    pub media_item_id: Uuid,
    pub source_rank: i32,
}

#[derive(Debug, Clone)]
struct DynamicCollectionRow {
    id: Uuid,
    library_id: Option<Uuid>,
    name: String,
    dynamic_config: serde_json::Value,
    sync_mode: String,
}

#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub builder_type: String,
    pub builder_data: serde_json::Value,
    pub limit: usize,
    pub minimum_items: usize,
    pub sort_by: String,
    pub title_format: Option<String>,
    pub include: HashSet<String>,
    pub exclude: HashSet<String>,
    pub key_name_override: HashMap<String, String>,
    pub remove_prefix: Vec<String>,
    pub remove_suffix: Vec<String>,
    pub selected_key: Option<String>,
}

pub async fn sync_dynamic_collection(
    pool: &PgPool,
    tmdb_client: Option<&TmdbClient>,
    collection_id: Uuid,
    include_external: bool,
    _reprocess_all: bool,
) -> Result<CollectionSyncResult, CollectionBuilderError> {
    let collection = load_dynamic_collection(pool, collection_id).await?;
    let config = BuilderConfig::from_value(&collection.dynamic_config)?;

    let candidates = build_candidates(
        pool,
        tmdb_client,
        collection.library_id,
        &config,
        include_external,
    )
    .await?;
    let selected = select_candidate(&collection, &config, candidates);

    persist_collection_items(pool, &collection, &config, &selected).await
}

pub async fn sync_dynamic_collections(
    pool: &PgPool,
    tmdb_client: Option<&TmdbClient>,
    library_id: Option<Uuid>,
    include_external: bool,
    reprocess_all: bool,
) -> Result<Vec<CollectionSyncResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT id
           FROM collections
           WHERE is_dynamic = true
             AND is_enabled = true
             AND ($1::uuid IS NULL OR library_id = $1)
           ORDER BY sort_order, name"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let collection_id: Uuid = row.try_get("id")?;
        results.push(
            sync_dynamic_collection(
                pool,
                tmdb_client,
                collection_id,
                include_external,
                reprocess_all,
            )
            .await?,
        );
    }

    Ok(results)
}

pub async fn build_candidates(
    pool: &PgPool,
    tmdb_client: Option<&TmdbClient>,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
    include_external: bool,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    match config.builder_type.as_str() {
        "genre" => build_genre(pool, library_id, config).await,
        "decade" => build_decade(pool, library_id, config).await,
        "actor" => build_actor(pool, library_id, config).await,
        "director" => build_director(pool, library_id, config).await,
        "franchise" => build_franchise(pool, library_id, config).await,
        "resolution" => build_resolution(pool, library_id, config).await,
        "audio_codec" => build_audio_codec(pool, library_id, config).await,
        "tmdb_popular" | "tmdb_top_rated" | "tmdb_trending" | "tmdb_now_playing"
        | "tmdb_upcoming" => {
            if include_external {
                build_tmdb_chart(pool, tmdb_client, library_id, config).await
            } else {
                Ok(Vec::new())
            }
        }
        other => Err(CollectionBuilderError::InvalidConfig(format!(
            "builder_type {other} is not implemented in Phase 12 Task 6"
        ))),
    }
}

async fn load_dynamic_collection(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<DynamicCollectionRow, CollectionBuilderError> {
    let row = sqlx::query(
        r#"SELECT id, library_id, name, dynamic_config, sync_mode
           FROM collections
           WHERE id = $1 AND is_dynamic = true"#,
    )
    .bind(collection_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        CollectionBuilderError::InvalidConfig(format!(
            "collection {collection_id} is not a dynamic collection"
        ))
    })?;

    Ok(DynamicCollectionRow {
        id: row.try_get("id")?,
        library_id: row.try_get("library_id").ok().flatten(),
        name: row.try_get("name")?,
        dynamic_config: row.try_get("dynamic_config")?,
        sync_mode: row.try_get("sync_mode")?,
    })
}

async fn build_genre(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT g.name AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY g.name
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM media_items mi
           JOIN media_genres mg ON mg.media_item_id = mi.id
           JOIN genres g ON g.id = mg.genre_id
           WHERE mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY g.name, source_rank"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("genre"))
}

async fn build_decade(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT ((EXTRACT(YEAR FROM mi.premiere_date)::int / 10) * 10)::text AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY ((EXTRACT(YEAR FROM mi.premiere_date)::int / 10) * 10)
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM media_items mi
           WHERE mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND mi.premiere_date IS NOT NULL
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY key_name, source_rank"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("decade"))
}

async fn build_actor(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let top_n = config
        .builder_data
        .get("top_n")
        .and_then(|v| v.as_u64())
        .unwrap_or(25)
        .clamp(1, 100) as i64;
    let minimum_appearances = config
        .builder_data
        .get("minimum_appearances")
        .and_then(|v| v.as_u64())
        .unwrap_or(3)
        .clamp(1, 100) as i64;

    let rows = sqlx::query(
        r#"WITH top_people AS (
               SELECT p.id, p.name, COUNT(DISTINCT mi.id) AS appearances
               FROM people p
               JOIN media_credits mc ON mc.person_id = p.id
               JOIN media_items mi ON mi.id = mc.media_item_id
               WHERE mc.credit_type = 'cast'
                 AND mi.type IN ('movie', 'series')
                 AND mi.match_state = 'confirmed'
                 AND ($1::uuid IS NULL OR mi.library_id = $1)
               GROUP BY p.id, p.name, p.sort_name
               HAVING COUNT(DISTINCT mi.id) >= $2
               ORDER BY appearances DESC, p.sort_name ASC
               LIMIT $3
           )
           SELECT tp.name AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY tp.id
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM top_people tp
           JOIN media_credits mc ON mc.person_id = tp.id
           JOIN media_items mi ON mi.id = mc.media_item_id
           WHERE mc.credit_type = 'cast'
             AND mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY tp.name, source_rank"#,
    )
    .bind(library_id)
    .bind(minimum_appearances)
    .bind(top_n)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("actor"))
}

async fn build_director(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let top_n = config
        .builder_data
        .get("top_n")
        .and_then(|v| v.as_u64())
        .unwrap_or(25)
        .clamp(1, 100) as i64;
    let minimum_appearances = config
        .builder_data
        .get("minimum_appearances")
        .and_then(|v| v.as_u64())
        .unwrap_or(2)
        .clamp(1, 100) as i64;

    let rows = sqlx::query(
        r#"WITH top_people AS (
               SELECT p.id, p.name, COUNT(DISTINCT mi.id) AS appearances
               FROM people p
               JOIN media_credits mc ON mc.person_id = p.id
               JOIN media_items mi ON mi.id = mc.media_item_id
               WHERE mc.credit_type = 'crew'
                 AND (mc.department = 'Directing' OR mc.role ILIKE '%director%')
                 AND mi.type IN ('movie', 'series')
                 AND mi.match_state = 'confirmed'
                 AND ($1::uuid IS NULL OR mi.library_id = $1)
               GROUP BY p.id, p.name, p.sort_name
               HAVING COUNT(DISTINCT mi.id) >= $2
               ORDER BY appearances DESC, p.sort_name ASC
               LIMIT $3
           )
           SELECT tp.name AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY tp.id
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM top_people tp
           JOIN media_credits mc ON mc.person_id = tp.id
           JOIN media_items mi ON mi.id = mc.media_item_id
           WHERE mc.credit_type = 'crew'
             AND (mc.department = 'Directing' OR mc.role ILIKE '%director%')
             AND mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY tp.name, source_rank"#,
    )
    .bind(library_id)
    .bind(minimum_appearances)
    .bind(top_n)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("director"))
}

async fn build_franchise(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT COALESCE(
                      mi.metadata #>> '{belongs_to_collection,name}',
                      mi.metadata #>> '{tmdb_collection,name}',
                      mi.metadata ->> 'collection_name',
                      mi.metadata ->> 'franchise'
                  ) AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY COALESCE(
                          mi.metadata #>> '{belongs_to_collection,name}',
                          mi.metadata #>> '{tmdb_collection,name}',
                          mi.metadata ->> 'collection_name',
                          mi.metadata ->> 'franchise'
                      )
                      ORDER BY mi.premiere_date ASC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM media_items mi
           WHERE mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
             AND COALESCE(
                 mi.metadata #>> '{belongs_to_collection,name}',
                 mi.metadata #>> '{tmdb_collection,name}',
                 mi.metadata ->> 'collection_name',
                 mi.metadata ->> 'franchise'
             ) IS NOT NULL
           ORDER BY key_name, source_rank"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("franchise"))
}

async fn build_resolution(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT mf.video_resolution AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY mf.video_resolution
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM media_items mi
           JOIN LATERAL (
               SELECT video_resolution
               FROM media_files
               WHERE media_item_id = mi.id
                 AND is_healthy = true
                 AND video_resolution IS NOT NULL
               ORDER BY file_size DESC
               LIMIT 1
           ) mf ON true
           WHERE mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY key_name, source_rank"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("resolution"))
}

async fn build_audio_codec(
    pool: &PgPool,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let rows = sqlx::query(
        r#"SELECT mf.audio_codec AS key_name,
                  mi.id AS media_item_id,
                  ROW_NUMBER() OVER (
                      PARTITION BY mf.audio_codec
                      ORDER BY mi.rating_average DESC NULLS LAST, mi.sort_title ASC
                  )::int AS source_rank
           FROM media_items mi
           JOIN LATERAL (
               SELECT audio_codec
               FROM media_files
               WHERE media_item_id = mi.id
                 AND is_healthy = true
                 AND audio_codec IS NOT NULL
               ORDER BY file_size DESC
               LIMIT 1
           ) mf ON true
           WHERE mi.type IN ('movie', 'series')
             AND mi.match_state = 'confirmed'
             AND ($1::uuid IS NULL OR mi.library_id = $1)
           ORDER BY key_name, source_rank"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    grouped_results(rows, config, default_title_format("audio_codec"))
}

async fn build_tmdb_chart(
    pool: &PgPool,
    tmdb_client: Option<&TmdbClient>,
    library_id: Option<Uuid>,
    config: &BuilderConfig,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let client = tmdb_client.ok_or_else(|| {
        CollectionBuilderError::ExternalUnavailable("TMDB is not configured".into())
    })?;

    let chart = tmdb_chart_from_builder(&config.builder_type)?;
    let media_types = tmdb_media_types(config, chart);
    let time_window = config
        .builder_data
        .get("time_window")
        .and_then(|v| v.as_str())
        .unwrap_or("day");
    let region = config.builder_data.get("region").and_then(|v| v.as_str());

    let chart_items = client
        .fetch_chart_items(chart, &media_types, config.limit, time_window, region)
        .await
        .map_err(map_metadata_error)?;

    let (matched, missing) = match_tmdb_items(pool, library_id, &chart_items).await?;
    let mut items = matched
        .into_iter()
        .take(config.limit)
        .enumerate()
        .map(|(idx, media_item_id)| CollectionBuilderItem {
            media_item_id,
            source_rank: (idx + 1) as i32,
        })
        .collect::<Vec<_>>();
    items.truncate(config.limit);

    if items.len() < config.minimum_items {
        return Ok(Vec::new());
    }

    let key = config.builder_type.clone();
    let name = config
        .title_format
        .as_deref()
        .map(|f| format_title(f, &key, "item", config.limit))
        .unwrap_or_else(|| tmdb_default_name(&config.builder_type).to_string());

    Ok(vec![CollectionBuilderResult {
        key,
        name,
        description: None,
        items,
        missing_external_ids: missing,
    }])
}

async fn match_tmdb_items(
    pool: &PgPool,
    library_id: Option<Uuid>,
    chart_items: &[TmdbChartItem],
) -> Result<(Vec<Uuid>, Vec<String>), CollectionBuilderError> {
    let movie_ids = chart_items
        .iter()
        .filter(|i| i.media_type == "movie")
        .map(|i| i.tmdb_id as i64)
        .collect::<Vec<_>>();
    let tv_ids = chart_items
        .iter()
        .filter(|i| i.media_type == "tv")
        .map(|i| i.tmdb_id as i64)
        .collect::<Vec<_>>();

    let rows = sqlx::query(
        r#"SELECT id, tmdb_id, type
           FROM media_items
           WHERE tmdb_id IS NOT NULL
             AND match_state = 'confirmed'
             AND ($1::uuid IS NULL OR library_id = $1)
             AND (
                 (type = 'movie' AND tmdb_id = ANY($2))
                 OR (type = 'series' AND tmdb_id = ANY($3))
             )"#,
    )
    .bind(library_id)
    .bind(&movie_ids)
    .bind(&tv_ids)
    .fetch_all(pool)
    .await?;

    let mut lookup = HashMap::new();
    for row in rows {
        let item_type: String = row.try_get("type")?;
        let media_type = if item_type == "series" { "tv" } else { "movie" };
        let tmdb_id: i64 = row.try_get("tmdb_id")?;
        let item_id: Uuid = row.try_get("id")?;
        lookup.insert((media_type.to_string(), tmdb_id as u64), item_id);
    }

    let mut seen = HashSet::new();
    let mut matched = Vec::new();
    let mut missing = Vec::new();

    for item in chart_items {
        let key = (item.media_type.clone(), item.tmdb_id);
        if let Some(item_id) = lookup.get(&key) {
            if seen.insert(*item_id) {
                matched.push(*item_id);
            }
        } else {
            missing.push(format!("{}:{}", item.media_type, item.tmdb_id));
        }
    }

    Ok((matched, missing))
}

fn grouped_results(
    rows: Vec<sqlx::postgres::PgRow>,
    config: &BuilderConfig,
    default_format: &str,
) -> Result<Vec<CollectionBuilderResult>, CollectionBuilderError> {
    let mut grouped: Vec<(String, Vec<CollectionBuilderItem>)> = Vec::new();
    let mut indexes: HashMap<String, usize> = HashMap::new();

    for row in rows {
        let Some(key) = row.try_get::<Option<String>, _>("key_name")? else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || !config.includes_key(key) {
            continue;
        }

        let idx = if let Some(idx) = indexes.get(key) {
            *idx
        } else {
            let idx = grouped.len();
            indexes.insert(key.to_string(), idx);
            grouped.push((key.to_string(), Vec::new()));
            idx
        };

        if grouped[idx].1.len() < config.limit {
            grouped[idx].1.push(CollectionBuilderItem {
                media_item_id: row.try_get("media_item_id")?,
                source_rank: row.try_get("source_rank")?,
            });
        }
    }

    let mut results = Vec::new();
    for (raw_key, items) in grouped {
        if items.len() < config.minimum_items {
            continue;
        }

        let display_key = config.display_key(&raw_key);
        let title_format = config.title_format.as_deref().unwrap_or(default_format);
        results.push(CollectionBuilderResult {
            key: raw_key,
            name: format_title(title_format, &display_key, "item", config.limit),
            description: None,
            items,
            missing_external_ids: Vec::new(),
        });
    }

    Ok(results)
}

fn select_candidate(
    collection: &DynamicCollectionRow,
    config: &BuilderConfig,
    candidates: Vec<CollectionBuilderResult>,
) -> CollectionBuilderResult {
    if candidates.is_empty() {
        return CollectionBuilderResult {
            key: config
                .selected_key
                .clone()
                .unwrap_or_else(|| config.builder_type.clone()),
            name: collection.name.clone(),
            description: None,
            items: Vec::new(),
            missing_external_ids: Vec::new(),
        };
    }

    if let Some(ref selected_key) = config.selected_key
        && let Some(candidate) = candidates
            .iter()
            .find(|c| c.key.eq_ignore_ascii_case(selected_key))
            .cloned()
    {
        return candidate;
    }

    if let Some(candidate) = candidates
        .iter()
        .find(|c| {
            c.name.eq_ignore_ascii_case(&collection.name)
                || c.key.eq_ignore_ascii_case(&collection.name)
        })
        .cloned()
    {
        return candidate;
    }

    if candidates.len() == 1 {
        return candidates.into_iter().next().unwrap();
    }

    let mut combined = Vec::new();
    for candidate in candidates {
        combined.extend(candidate.items);
    }
    dedupe_items(&mut combined);
    combined.truncate(config.limit);

    CollectionBuilderResult {
        key: config.builder_type.clone(),
        name: collection.name.clone(),
        description: None,
        items: combined,
        missing_external_ids: Vec::new(),
    }
}

async fn persist_collection_items(
    pool: &PgPool,
    collection: &DynamicCollectionRow,
    config: &BuilderConfig,
    result: &CollectionBuilderResult,
) -> Result<CollectionSyncResult, CollectionBuilderError> {
    let before_ids = collection_item_ids(pool, collection.id).await?;
    let before_set = before_ids.iter().copied().collect::<HashSet<_>>();
    let desired_set = result
        .items
        .iter()
        .map(|i| i.media_item_id)
        .collect::<HashSet<_>>();
    let added = desired_set.difference(&before_set).count();
    let removed = if collection.sync_mode == "sync" {
        before_set.difference(&desired_set).count()
    } else {
        0
    };

    if collection.sync_mode == "sync" {
        if result.items.is_empty() {
            sqlx::query("DELETE FROM collection_items WHERE collection_id = $1")
                .bind(collection.id)
                .execute(pool)
                .await?;
        } else {
            let keep_ids = result
                .items
                .iter()
                .map(|i| i.media_item_id)
                .collect::<Vec<_>>();
            sqlx::query(
                "DELETE FROM collection_items WHERE collection_id = $1 AND NOT (media_item_id = ANY($2))",
            )
            .bind(collection.id)
            .bind(&keep_ids)
            .execute(pool)
            .await?;
        }
    }

    for item in &result.items {
        sqlx::query(
            r#"INSERT INTO collection_items (collection_id, media_item_id, position, is_missing)
               VALUES ($1, $2, $3, false)
               ON CONFLICT (collection_id, media_item_id)
               DO UPDATE SET position = EXCLUDED.position, is_missing = false, missing_reason = NULL"#,
        )
        .bind(collection.id)
        .bind(item.media_item_id)
        .bind(item.source_rank * 1000)
        .execute(pool)
        .await?;
    }

    let after_count = count_collection_items(pool, collection.id).await?;
    let total_duration_seconds = total_collection_duration(pool, collection.id).await?;

    let sync_result = serde_json::json!({
        "builder_type": config.builder_type,
        "builder_key": result.key,
        "added": added,
        "removed": removed,
        "missing": result.missing_external_ids.len(),
        "total_matched": result.items.len(),
        "synced_at": Utc::now(),
    });

    sqlx::query(
        r#"UPDATE collections
           SET item_count = $2,
               total_duration_seconds = $3,
               last_synced_at = now(),
               last_sync_result = $4,
               updated_at = now()
           WHERE id = $1"#,
    )
    .bind(collection.id)
    .bind(after_count as i32)
    .bind(total_duration_seconds)
    .bind(sync_result)
    .execute(pool)
    .await?;

    Ok(CollectionSyncResult {
        collection_id: collection.id,
        builder_type: config.builder_type.clone(),
        added,
        removed,
        total_matched: result.items.len(),
        missing: result.missing_external_ids.len(),
    })
}

async fn collection_item_ids(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<Vec<Uuid>, CollectionBuilderError> {
    let rows = sqlx::query(
        "SELECT media_item_id FROM collection_items WHERE collection_id = $1 AND is_missing = false",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| row.try_get("media_item_id"))
        .collect::<Result<Vec<_>, _>>()
        .map_err(CollectionBuilderError::Database)
}

async fn count_collection_items(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<usize, CollectionBuilderError> {
    let row = sqlx::query(
        "SELECT COUNT(*)::bigint AS count FROM collection_items WHERE collection_id = $1",
    )
    .bind(collection_id)
    .fetch_one(pool)
    .await?;
    let count: i64 = row.try_get("count")?;
    Ok(count.max(0) as usize)
}

async fn total_collection_duration(
    pool: &PgPool,
    collection_id: Uuid,
) -> Result<i32, CollectionBuilderError> {
    let row = sqlx::query(
        r#"SELECT COALESCE(SUM(mi.runtime_seconds), 0)::int AS total_duration_seconds
           FROM collection_items ci
           JOIN media_items mi ON mi.id = ci.media_item_id
           WHERE ci.collection_id = $1"#,
    )
    .bind(collection_id)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("total_duration_seconds")?)
}

fn dedupe_items(items: &mut Vec<CollectionBuilderItem>) {
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(i.media_item_id));
    for (idx, item) in items.iter_mut().enumerate() {
        item.source_rank = (idx + 1) as i32;
    }
}

fn tmdb_chart_from_builder(builder_type: &str) -> Result<TmdbChart, CollectionBuilderError> {
    match builder_type {
        "tmdb_popular" => Ok(TmdbChart::Popular),
        "tmdb_top_rated" => Ok(TmdbChart::TopRated),
        "tmdb_trending" => Ok(TmdbChart::Trending),
        "tmdb_now_playing" => Ok(TmdbChart::NowPlaying),
        "tmdb_upcoming" => Ok(TmdbChart::Upcoming),
        _ => Err(CollectionBuilderError::InvalidConfig(format!(
            "unsupported TMDB builder_type: {builder_type}"
        ))),
    }
}

fn tmdb_media_types(config: &BuilderConfig, chart: TmdbChart) -> Vec<&'static str> {
    if matches!(chart, TmdbChart::NowPlaying | TmdbChart::Upcoming) {
        return vec!["movie"];
    }

    if let Some(media_type) = config
        .builder_data
        .get("media_type")
        .and_then(|v| v.as_str())
    {
        return match media_type {
            "movie" | "movies" => vec!["movie"],
            "tv" | "series" | "shows" => vec!["tv"],
            _ => vec!["movie", "tv"],
        };
    }

    vec!["movie", "tv"]
}

fn map_metadata_error(err: MetadataError) -> CollectionBuilderError {
    match err {
        MetadataError::RateLimited { .. } => CollectionBuilderError::ExternalRateLimited,
        MetadataError::Database(e) => CollectionBuilderError::Database(e),
        e => CollectionBuilderError::ExternalUnavailable(e.to_string()),
    }
}

fn default_title_format(builder_type: &str) -> &'static str {
    match builder_type {
        "genre" => "<<key_name>>",
        "decade" => "<<key_name>>s",
        "actor" => "<<key_name>>",
        "director" => "<<key_name>>",
        "franchise" => "<<key_name>>",
        "resolution" => "<<key_name>>",
        "audio_codec" => "<<key_name>>",
        _ => "<<key_name>>",
    }
}

fn tmdb_default_name(builder_type: &str) -> &'static str {
    match builder_type {
        "tmdb_popular" => "TMDb Popular",
        "tmdb_top_rated" => "TMDb Top Rated",
        "tmdb_trending" => "Trending on TMDb",
        "tmdb_now_playing" => "Now Playing",
        "tmdb_upcoming" => "Upcoming",
        _ => "TMDb Collection",
    }
}

fn format_title(format: &str, key_name: &str, library_type: &str, limit: usize) -> String {
    format
        .replace("<<key_name>>", key_name)
        .replace("<<library_type>>", library_type)
        .replace("<<limit>>", &limit.to_string())
}

impl BuilderConfig {
    pub fn from_value(value: &serde_json::Value) -> Result<Self, CollectionBuilderError> {
        let builder_type = value
            .get("builder_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                CollectionBuilderError::InvalidConfig("builder_type is required".into())
            })?
            .to_string();

        let builder_data = value
            .get("builder_data")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));

        let limit = value
            .get("limit")
            .or_else(|| builder_data.get("limit"))
            .and_then(|v| v.as_u64())
            .unwrap_or(100)
            .clamp(1, 500) as usize;

        let minimum_items = value
            .get("minimum_items")
            .or_else(|| builder_data.get("minimum_items"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 500) as usize;

        Ok(Self {
            builder_type,
            limit,
            minimum_items,
            sort_by: value
                .get("sort_by")
                .and_then(|v| v.as_str())
                .unwrap_or("rating_average.desc")
                .to_string(),
            title_format: value
                .get("title_format")
                .and_then(|v| v.as_str())
                .map(String::from),
            include: string_set(value.get("include")),
            exclude: string_set(value.get("exclude")),
            key_name_override: string_map(value.get("key_name_override")),
            remove_prefix: string_vec(value.get("remove_prefix")),
            remove_suffix: string_vec(value.get("remove_suffix")),
            selected_key: value
                .get("key")
                .or_else(|| builder_data.get("key"))
                .and_then(|v| v.as_str())
                .map(String::from),
            builder_data,
        })
    }

    fn includes_key(&self, key: &str) -> bool {
        let normalized = normalize_key(key);
        (self.include.is_empty() || self.include.contains(&normalized))
            && !self.exclude.contains(&normalized)
    }

    fn display_key(&self, key: &str) -> String {
        if let Some(value) = self.key_name_override.get(key) {
            return value.clone();
        }

        let mut value = key.to_string();
        for prefix in &self.remove_prefix {
            let trimmed = value.trim_start();
            if trimmed
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
            {
                value = trimmed[prefix.len()..].trim_start().to_string();
            }
        }
        for suffix in &self.remove_suffix {
            let trimmed = value.trim_end();
            if trimmed
                .to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
            {
                let keep_len = trimmed.len().saturating_sub(suffix.len());
                value = trimmed[..keep_len].trim_end().to_string();
            }
        }
        value
    }
}

fn string_set(value: Option<&serde_json::Value>) -> HashSet<String> {
    string_vec(value)
        .into_iter()
        .map(|s| normalize_key(&s))
        .collect()
}

fn string_vec(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
        Some(serde_json::Value::String(value)) => vec![value.clone()],
        _ => Vec::new(),
    }
}

fn string_map(value: Option<&serde_json::Value>) -> HashMap<String, String> {
    value
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_builder_config_defaults() {
        let value = serde_json::json!({
            "builder_type": "genre"
        });

        let config = BuilderConfig::from_value(&value).unwrap();

        assert_eq!(config.builder_type, "genre");
        assert_eq!(config.limit, 100);
        assert_eq!(config.minimum_items, 1);
        assert_eq!(config.sort_by, "rating_average.desc");
    }

    #[test]
    fn parses_limit_from_builder_data() {
        let value = serde_json::json!({
            "builder_type": "tmdb_trending",
            "builder_data": {
                "limit": 20,
                "time_window": "week"
            }
        });

        let config = BuilderConfig::from_value(&value).unwrap();

        assert_eq!(config.limit, 20);
        assert_eq!(config.builder_data["time_window"], "week");
    }

    #[test]
    fn include_exclude_are_case_insensitive() {
        let value = serde_json::json!({
            "builder_type": "genre",
            "include": ["Action", "Drama"],
            "exclude": ["Talk Show"]
        });

        let config = BuilderConfig::from_value(&value).unwrap();

        assert!(config.includes_key("action"));
        assert!(config.includes_key("DRAMA"));
        assert!(!config.includes_key("Comedy"));
        assert!(!config.includes_key("talk show"));
    }

    #[test]
    fn display_key_applies_override_before_cleanup() {
        let value = serde_json::json!({
            "builder_type": "franchise",
            "key_name_override": {
                "Star Wars Collection": "Star Wars Universe"
            },
            "remove_suffix": ["Collection"]
        });

        let config = BuilderConfig::from_value(&value).unwrap();

        assert_eq!(
            config.display_key("Star Wars Collection"),
            "Star Wars Universe"
        );
        assert_eq!(config.display_key("Alien Collection"), "Alien");
    }

    #[test]
    fn title_format_replaces_supported_variables() {
        assert_eq!(
            format_title("Top <<key_name>> <<library_type>>s", "Action", "movie", 50),
            "Top Action movies"
        );
        assert_eq!(
            format_title("Top <<limit>>", "Action", "movie", 25),
            "Top 25"
        );
    }

    #[test]
    fn tmdb_chart_mapping_rejects_unknown_builder() {
        assert_eq!(
            tmdb_chart_from_builder("tmdb_popular").unwrap(),
            TmdbChart::Popular
        );
        assert!(tmdb_chart_from_builder("trakt_popular").is_err());
    }

    #[test]
    fn tmdb_now_playing_is_movie_only() {
        let value = serde_json::json!({
            "builder_type": "tmdb_now_playing",
            "builder_data": { "media_type": "both" }
        });
        let config = BuilderConfig::from_value(&value).unwrap();

        assert_eq!(
            tmdb_media_types(&config, TmdbChart::NowPlaying),
            vec!["movie"]
        );
    }

    #[test]
    fn select_candidate_prefers_configured_key() {
        let collection = DynamicCollectionRow {
            id: Uuid::nil(),
            library_id: None,
            name: "Action".into(),
            dynamic_config: serde_json::json!({}),
            sync_mode: "sync".into(),
        };
        let mut config = BuilderConfig::from_value(&serde_json::json!({
            "builder_type": "genre",
            "key": "Drama"
        }))
        .unwrap();
        config.limit = 10;
        let candidates = vec![
            CollectionBuilderResult {
                key: "Action".into(),
                name: "Action".into(),
                description: None,
                items: Vec::new(),
                missing_external_ids: Vec::new(),
            },
            CollectionBuilderResult {
                key: "Drama".into(),
                name: "Drama".into(),
                description: None,
                items: Vec::new(),
                missing_external_ids: Vec::new(),
            },
        ];

        let selected = select_candidate(&collection, &config, candidates);

        assert_eq!(selected.key, "Drama");
    }

    #[test]
    fn dedupe_items_preserves_first_seen_order() {
        let first = Uuid::now_v7();
        let second = Uuid::now_v7();
        let mut items = vec![
            CollectionBuilderItem {
                media_item_id: first,
                source_rank: 4,
            },
            CollectionBuilderItem {
                media_item_id: second,
                source_rank: 8,
            },
            CollectionBuilderItem {
                media_item_id: first,
                source_rank: 10,
            },
        ];

        dedupe_items(&mut items);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].media_item_id, first);
        assert_eq!(items[0].source_rank, 1);
        assert_eq!(items[1].media_item_id, second);
        assert_eq!(items[1].source_rank, 2);
    }

    #[test]
    fn string_vec_accepts_string_or_array() {
        assert_eq!(
            string_vec(Some(&serde_json::json!("Action"))),
            vec!["Action".to_string()]
        );
        assert_eq!(
            string_vec(Some(&serde_json::json!(["Action", "Drama"]))),
            vec!["Action".to_string(), "Drama".to_string()]
        );
    }

    #[test]
    fn sync_result_serializes_for_last_sync_result() {
        let result = CollectionSyncResult {
            collection_id: Uuid::nil(),
            builder_type: "genre".into(),
            added: 2,
            removed: 1,
            total_matched: 10,
            missing: 0,
        };

        let value = serde_json::to_value(result).unwrap();

        assert_eq!(value["builder_type"], "genre");
        assert_eq!(value["added"], 2);
    }
}
