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

use base64::Engine;
use chrono::{DateTime, Datelike, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::domains::profiles::types::ProfileScope;

use super::access::scoped_media_query;
use super::error::MediaError;
use super::projection::{BROWSE_SELECT_SQL, browse_row_to_response};
use super::service::validate_media_type;
use super::types::{MediaBrowseListResponse, MediaBrowsePageQuery, MediaBrowseQuery};

#[derive(Clone, Copy)]
enum BrowseMode {
    Catalog,
    Continue,
    Seasons(Uuid),
    Episodes(Uuid),
    Collection(Uuid),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum BrowseCursorKey {
    Added,
    Title(String),
    Number(Option<i32>),
    Activity(DateTime<Utc>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BrowseCursor {
    version: u8,
    context: String,
    id: Uuid,
    key: BrowseCursorKey,
}

struct BrowseOptions<'a> {
    limit: u32,
    sort: &'a str,
    order: &'a str,
    watch: &'a str,
}

fn browse_options(query: &MediaBrowseQuery) -> Result<BrowseOptions<'_>, MediaError> {
    let limit = query.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(MediaError::InvalidBrowseQuery(
            "limit must be between 1 and 100".into(),
        ));
    }
    if let Some(value) = query.r#type.as_deref() {
        validate_media_type(value)?;
    }
    let sort = query.sort.as_deref().unwrap_or("added");
    if !["added", "title", "year"].contains(&sort) {
        return Err(MediaError::InvalidBrowseQuery(
            "sort must be added, title, or year".into(),
        ));
    }
    let order = query.order.as_deref().unwrap_or("desc");
    if !["asc", "desc"].contains(&order) {
        return Err(MediaError::InvalidBrowseQuery(
            "order must be asc or desc".into(),
        ));
    }
    let watch = query.watch.as_deref().unwrap_or("all");
    if !["all", "watched", "unwatched", "in_progress"].contains(&watch) {
        return Err(MediaError::InvalidBrowseQuery(
            "unknown watch filter".into(),
        ));
    }
    if query.year.is_some_and(|year| !(1..=9999).contains(&year)) {
        return Err(MediaError::InvalidBrowseQuery(
            "year must be between 1 and 9999".into(),
        ));
    }
    Ok(BrowseOptions {
        limit,
        sort,
        order,
        watch,
    })
}

fn browse_context(
    scope: &ProfileScope,
    mode: BrowseMode,
    query: &MediaBrowseQuery,
    options: &BrowseOptions<'_>,
) -> String {
    let (route, parent) = match mode {
        BrowseMode::Catalog => ("catalog", None),
        BrowseMode::Continue => ("continue", None),
        BrowseMode::Seasons(id) => ("seasons", Some(id)),
        BrowseMode::Episodes(id) => ("episodes", Some(id)),
        BrowseMode::Collection(id) => ("collection", Some(id)),
    };
    serde_json::json!([
        scope.owner_user_id,
        scope.profile_id,
        route,
        parent,
        query.library_id,
        query.r#type,
        options.sort,
        options.order,
        options.watch,
        query.favorite,
        query.genre_id,
        query.year
    ])
    .to_string()
}

fn decode_browse_cursor(
    value: Option<&str>,
    context: &str,
    allow_legacy: bool,
) -> Result<Option<BrowseCursor>, MediaError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.len() > 8192 {
        return Err(MediaError::InvalidBrowseCursor);
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| MediaError::InvalidBrowseCursor)?;
    if let Ok(cursor) = serde_json::from_slice::<BrowseCursor>(&bytes) {
        if cursor.version != 1 || cursor.context != context {
            return Err(MediaError::InvalidBrowseCursor);
        }
        return Ok(Some(cursor));
    }
    if allow_legacy {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct LegacyCursor {
            id: Uuid,
        }
        if let Ok(cursor) = serde_json::from_slice::<LegacyCursor>(&bytes) {
            return Ok(Some(BrowseCursor {
                version: 1,
                context: context.into(),
                id: cursor.id,
                key: BrowseCursorKey::Added,
            }));
        }
    }
    Err(MediaError::InvalidBrowseCursor)
}

pub async fn browse_media_items(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    query: &MediaBrowseQuery,
) -> Result<MediaBrowseListResponse, MediaError> {
    browse_items(pool, scope, query, BrowseMode::Catalog).await
}

pub async fn browse_collection_items(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    collection_id: Uuid,
    query: &MediaBrowseQuery,
) -> Result<MediaBrowseListResponse, MediaError> {
    browse_items(pool, scope, query, BrowseMode::Collection(collection_id)).await
}

pub async fn browse_continue_watching(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    query: &MediaBrowsePageQuery,
) -> Result<MediaBrowseListResponse, MediaError> {
    browse_items(
        pool,
        scope,
        &MediaBrowseQuery {
            limit: query.limit,
            cursor: query.cursor.clone(),
            ..Default::default()
        },
        BrowseMode::Continue,
    )
    .await
}

pub async fn browse_series_seasons(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    series_id: Uuid,
    query: &MediaBrowsePageQuery,
) -> Result<MediaBrowseListResponse, MediaError> {
    verify_browse_parent(pool, scope, series_id, "series").await?;
    browse_items(
        pool,
        scope,
        &MediaBrowseQuery {
            limit: query.limit,
            cursor: query.cursor.clone(),
            ..Default::default()
        },
        BrowseMode::Seasons(series_id),
    )
    .await
}

pub async fn browse_season_episodes(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    season_id: Uuid,
    query: &MediaBrowsePageQuery,
) -> Result<MediaBrowseListResponse, MediaError> {
    verify_browse_parent(pool, scope, season_id, "season").await?;
    browse_items(
        pool,
        scope,
        &MediaBrowseQuery {
            limit: query.limit,
            cursor: query.cursor.clone(),
            ..Default::default()
        },
        BrowseMode::Episodes(season_id),
    )
    .await
}

async fn verify_browse_parent(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    id: Uuid,
    item_type: &str,
) -> Result<(), MediaError> {
    let mut sql = scoped_media_query(scope);
    sql.push("SELECT id FROM allowed_media WHERE id = ")
        .push_bind(id)
        .push(" AND type = ")
        .push_bind(item_type.to_string());
    if sql.build().fetch_optional(pool).await?.is_none() {
        return Err(if item_type == "series" {
            MediaError::SeriesNotFound
        } else {
            MediaError::SeasonNotFound
        });
    }
    Ok(())
}

async fn browse_items(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    query: &MediaBrowseQuery,
    mode: BrowseMode,
) -> Result<MediaBrowseListResponse, MediaError> {
    let mut options = browse_options(query)?;
    match mode {
        BrowseMode::Continue => {
            options.sort = "activity";
            options.order = "desc";
        }
        BrowseMode::Seasons(_) | BrowseMode::Episodes(_) => {
            options.sort = "number";
            options.order = "asc";
        }
        BrowseMode::Collection(_) if query.sort.is_none() => {
            options.sort = "position";
            options.order = query.order.as_deref().unwrap_or("asc");
        }
        BrowseMode::Catalog | BrowseMode::Collection(_) => {}
    }
    let context = browse_context(scope, mode, query, &options);
    let cursor = decode_browse_cursor(
        query.cursor.as_deref(),
        &context,
        matches!(mode, BrowseMode::Catalog) && options.sort == "added",
    )?;
    let mut sql = scoped_media_query(scope);
    if matches!(mode, BrowseMode::Collection(_)) {
        sql.push("SELECT ci.position AS collection_position, ")
            .push(
                BROWSE_SELECT_SQL
                    .strip_prefix("SELECT ")
                    .expect("browse select prefix"),
            );
    } else {
        sql.push(BROWSE_SELECT_SQL);
    }
    sql.push_bind(scope.profile_id)
        .push(" AND uid.user_id = ")
        .push_bind(scope.owner_user_id);
    if let BrowseMode::Collection(id) = mode {
        sql.push(" JOIN collection_items ci ON ci.media_item_id = mi.id AND ci.collection_id = ")
            .push_bind(id).push(" AND ci.is_missing = false JOIN collections collection ON collection.id = ci.collection_id");
    }
    super::projection::push_browse_children(&mut sql, scope);
    match mode {
        BrowseMode::Catalog | BrowseMode::Collection(_) => {
            if matches!(mode, BrowseMode::Collection(_)) {
                sql.push(" AND collection.is_enabled AND collection.visibility IN ('visible', 'featured') AND (collection.library_id IS NULL OR (true");
                super::access::push_library_scope(&mut sql, scope, "collection.library_id");
                sql.push("))");
            }
            if let Some(id) = query.library_id {
                sql.push(" AND mi.library_id = ").push_bind(id);
            }
            if let Some(item_type) = &query.r#type {
                sql.push(" AND mi.type = ").push_bind(item_type.clone());
            }
            if let Some(genre) = query.genre_id {
                sql.push(" AND EXISTS (SELECT 1 FROM media_genres genre WHERE genre.media_item_id = mi.id AND genre.genre_id = ").push_bind(genre).push(")");
            }
            if let Some(year) = query.year {
                sql.push(" AND EXTRACT(YEAR FROM mi.premiere_date)::int = ")
                    .push_bind(year);
            }
            if let Some(favorite) = query.favorite {
                sql.push(" AND COALESCE(uid.is_favorite, false) = ")
                    .push_bind(favorite);
            }
            match options.watch {
                "watched" => {
                    sql.push(" AND COALESCE(uid.is_watched, false) = true");
                }
                "unwatched" => {
                    sql.push(" AND COALESCE(uid.is_watched, false) = false");
                }
                "in_progress" => {
                    sql.push(" AND COALESCE(uid.is_watched, false) = false AND uid.resume_position_ms > 0 AND uid.last_played_at IS NOT NULL");
                }
                _ => {}
            }
        }
        BrowseMode::Continue => {
            sql.push(" AND mi.type IN ('movie', 'episode') AND uid.is_watched = false AND uid.resume_position_ms > 0 AND uid.last_played_at IS NOT NULL");
        }
        BrowseMode::Seasons(id) => {
            sql.push(" AND mi.type = 'season' AND sn.series_id = ")
                .push_bind(id);
        }
        BrowseMode::Episodes(id) => {
            sql.push(" AND mi.type = 'episode' AND ep.season_id = ")
                .push_bind(id);
        }
    }
    let number_column = match mode {
        BrowseMode::Seasons(_) => "sn.season_number",
        BrowseMode::Episodes(_) => "ep.episode_number",
        BrowseMode::Collection(_) if options.sort == "position" => "ci.position",
        _ => "EXTRACT(YEAR FROM mi.premiere_date)::int",
    };
    if let Some(cursor) = &cursor {
        push_browse_cursor(&mut sql, cursor, &options, number_column)?;
    }
    let direction = if options.order == "asc" {
        " ASC"
    } else {
        " DESC"
    };
    sql.push(" ORDER BY ");
    match options.sort {
        "added" => {}
        "title" => {
            sql.push("mi.sort_title").push(direction).push(", ");
        }
        "activity" => {
            sql.push("uid.last_played_at DESC, ");
        }
        "year" | "number" | "position" => {
            sql.push(number_column)
                .push(direction)
                .push(" NULLS LAST, ");
        }
        _ => unreachable!(),
    }
    sql.push("mi.id")
        .push(direction)
        .push(" LIMIT ")
        .push_bind(i64::from(options.limit) + 1);
    let rows = sql.build().fetch_all(pool).await?;
    let has_more = rows.len() > options.limit as usize;
    let items: Vec<_> = rows
        .iter()
        .take(options.limit as usize)
        .map(browse_row_to_response)
        .collect();
    let next_cursor = if has_more {
        items.last().map(|item| {
            let key = match options.sort {
                "added" => BrowseCursorKey::Added,
                "title" => BrowseCursorKey::Title(item.media.sort_title.clone()),
                "activity" => BrowseCursorKey::Activity(
                    item.watch_state
                        .last_played_at
                        .expect("continue query requires activity"),
                ),
                "year" => BrowseCursorKey::Number(item.media.premiere_date.map(|date| date.year())),
                "position" => {
                    BrowseCursorKey::Number(Some(rows[items.len() - 1].get("collection_position")))
                }
                "number" => BrowseCursorKey::Number(if matches!(mode, BrowseMode::Seasons(_)) {
                    item.media.season_number
                } else {
                    item.media.episode_number
                }),
                _ => unreachable!(),
            };
            base64::engine::general_purpose::STANDARD.encode(
                serde_json::to_vec(&BrowseCursor {
                    version: 1,
                    context: context.clone(),
                    id: item.media.id,
                    key,
                })
                .expect("serializable browsing cursor"),
            )
        })
    } else {
        None
    };
    Ok(MediaBrowseListResponse {
        items,
        cursor: next_cursor,
        has_more,
    })
}

fn push_browse_cursor(
    sql: &mut QueryBuilder<Postgres>,
    cursor: &BrowseCursor,
    options: &BrowseOptions<'_>,
    number_column: &str,
) -> Result<(), MediaError> {
    let comparator = if options.order == "asc" { " > " } else { " < " };
    match (&cursor.key, options.sort) {
        (BrowseCursorKey::Added, "added") => {
            sql.push(" AND mi.id").push(comparator).push_bind(cursor.id);
        }
        (BrowseCursorKey::Title(title), "title") => {
            sql.push(" AND (mi.sort_title, mi.id)")
                .push(comparator)
                .push("(")
                .push_bind(title.clone())
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        (BrowseCursorKey::Activity(activity), "activity") => {
            sql.push(" AND (uid.last_played_at, mi.id) < (")
                .push_bind(*activity)
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        (BrowseCursorKey::Number(number), "year" | "number" | "position") => {
            if let Some(number) = number {
                sql.push(" AND (")
                    .push(number_column)
                    .push(comparator)
                    .push_bind(*number)
                    .push(" OR (")
                    .push(number_column)
                    .push(" = ")
                    .push_bind(*number)
                    .push(" AND mi.id")
                    .push(comparator)
                    .push_bind(cursor.id)
                    .push(") OR ")
                    .push(number_column)
                    .push(" IS NULL)");
            } else {
                sql.push(" AND ")
                    .push(number_column)
                    .push(" IS NULL AND mi.id")
                    .push(comparator)
                    .push_bind(cursor.id);
            }
        }
        _ => return Err(MediaError::InvalidBrowseCursor),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
