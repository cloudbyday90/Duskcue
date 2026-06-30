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
use std::time::Instant;

use crate::domains::media::service::{row_to_response, validate_media_type};
use crate::extractors::AuthenticatedUser;

use super::error::SearchError;
use super::types::{SearchFacetCount, SearchFacets, SearchParams, SearchQuery, SearchResponse};

const MAX_QUERY_LEN: usize = 200;
const DEFAULT_LIMIT: u32 = 40;
const MAX_LIMIT: u32 = 100;

const SEARCH_ITEMS_SQL: &str = r#"
WITH search_query AS (SELECT plainto_tsquery('english', $1) AS query)
SELECT mi.*, s.status as series_status,
       sn.series_id, sn.season_number, sn.id as season_id,
       ep.episode_number, ep.absolute_episode_number,
       COALESCE(mf.cnt, 0) as file_count
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
CROSS JOIN search_query sq
LEFT JOIN series s ON s.id = mi.id
LEFT JOIN seasons sn ON sn.id = mi.id
LEFT JOIN episodes ep ON ep.id = mi.id
LEFT JOIN LATERAL (
    SELECT count(*) as cnt FROM media_files mf WHERE mf.media_item_id = mi.id
) mf ON true
WHERE mi.search_vector @@ sq.query
  AND l.deleted_at IS NULL
  AND ($2::text IS NULL OR mi.type = $2)
  AND ($3::text IS NULL OR EXISTS (
      SELECT 1
      FROM media_genres mg
      JOIN genres g ON g.id = mg.genre_id
      WHERE mg.media_item_id = mi.id AND g.slug = $3
  ))
  AND ($4::int IS NULL OR EXTRACT(YEAR FROM mi.premiere_date)::int = $4)
  AND ($5::real IS NULL OR mi.rating_average >= $5)
  AND ($6::bool OR EXISTS (
      SELECT 1
      FROM user_library_access ula
      WHERE ula.user_id = $7 AND ula.library_id = mi.library_id
  ))
ORDER BY ts_rank(mi.search_vector, sq.query) DESC, mi.sort_title ASC
LIMIT $8
"#;

const TYPE_FACETS_SQL: &str = r#"
WITH search_query AS (SELECT plainto_tsquery('english', $1) AS query)
SELECT mi.type as value, mi.type as label, count(*)::bigint as count
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
CROSS JOIN search_query sq
WHERE mi.search_vector @@ sq.query
  AND l.deleted_at IS NULL
  AND ($2::text IS NULL OR mi.type = $2)
  AND ($3::text IS NULL OR EXISTS (
      SELECT 1
      FROM media_genres mg
      JOIN genres g ON g.id = mg.genre_id
      WHERE mg.media_item_id = mi.id AND g.slug = $3
  ))
  AND ($4::int IS NULL OR EXTRACT(YEAR FROM mi.premiere_date)::int = $4)
  AND ($5::real IS NULL OR mi.rating_average >= $5)
  AND ($6::bool OR EXISTS (
      SELECT 1
      FROM user_library_access ula
      WHERE ula.user_id = $7 AND ula.library_id = mi.library_id
  ))
GROUP BY mi.type
ORDER BY count DESC, mi.type ASC
"#;

const GENRE_FACETS_SQL: &str = r#"
WITH search_query AS (SELECT plainto_tsquery('english', $1) AS query)
SELECT g.slug as value, g.name as label, count(*)::bigint as count
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
JOIN media_genres mg ON mg.media_item_id = mi.id
JOIN genres g ON g.id = mg.genre_id
CROSS JOIN search_query sq
WHERE mi.search_vector @@ sq.query
  AND l.deleted_at IS NULL
  AND ($2::text IS NULL OR mi.type = $2)
  AND ($3::text IS NULL OR EXISTS (
      SELECT 1
      FROM media_genres mg_filter
      JOIN genres g_filter ON g_filter.id = mg_filter.genre_id
      WHERE mg_filter.media_item_id = mi.id AND g_filter.slug = $3
  ))
  AND ($4::int IS NULL OR EXTRACT(YEAR FROM mi.premiere_date)::int = $4)
  AND ($5::real IS NULL OR mi.rating_average >= $5)
  AND ($6::bool OR EXISTS (
      SELECT 1
      FROM user_library_access ula
      WHERE ula.user_id = $7 AND ula.library_id = mi.library_id
  ))
GROUP BY g.slug, g.name
ORDER BY count DESC, g.name ASC
LIMIT 16
"#;

const YEAR_FACETS_SQL: &str = r#"
WITH search_query AS (SELECT plainto_tsquery('english', $1) AS query)
SELECT EXTRACT(YEAR FROM mi.premiere_date)::int::text as value,
       EXTRACT(YEAR FROM mi.premiere_date)::int::text as label,
       count(*)::bigint as count
FROM media_items mi
JOIN libraries l ON l.id = mi.library_id
CROSS JOIN search_query sq
WHERE mi.search_vector @@ sq.query
  AND l.deleted_at IS NULL
  AND mi.premiere_date IS NOT NULL
  AND ($2::text IS NULL OR mi.type = $2)
  AND ($3::text IS NULL OR EXISTS (
      SELECT 1
      FROM media_genres mg
      JOIN genres g ON g.id = mg.genre_id
      WHERE mg.media_item_id = mi.id AND g.slug = $3
  ))
  AND ($4::int IS NULL OR EXTRACT(YEAR FROM mi.premiere_date)::int = $4)
  AND ($5::real IS NULL OR mi.rating_average >= $5)
  AND ($6::bool OR EXISTS (
      SELECT 1
      FROM user_library_access ula
      WHERE ula.user_id = $7 AND ula.library_id = mi.library_id
  ))
GROUP BY EXTRACT(YEAR FROM mi.premiere_date)::int
ORDER BY EXTRACT(YEAR FROM mi.premiere_date)::int DESC
LIMIT 16
"#;

const RATING_FACETS_SQL: &str = r#"
WITH search_query AS (SELECT plainto_tsquery('english', $1) AS query),
matched AS (
    SELECT mi.rating_average
    FROM media_items mi
    JOIN libraries l ON l.id = mi.library_id
    CROSS JOIN search_query sq
    WHERE mi.search_vector @@ sq.query
      AND l.deleted_at IS NULL
      AND mi.rating_average IS NOT NULL
      AND mi.rating_average >= 6
      AND ($2::text IS NULL OR mi.type = $2)
      AND ($3::text IS NULL OR EXISTS (
          SELECT 1
          FROM media_genres mg
          JOIN genres g ON g.id = mg.genre_id
          WHERE mg.media_item_id = mi.id AND g.slug = $3
      ))
      AND ($4::int IS NULL OR EXTRACT(YEAR FROM mi.premiere_date)::int = $4)
      AND ($5::real IS NULL OR mi.rating_average >= $5)
      AND ($6::bool OR EXISTS (
          SELECT 1
          FROM user_library_access ula
          WHERE ula.user_id = $7 AND ula.library_id = mi.library_id
      ))
)
SELECT bucket::text as value, bucket::text || '+' as label, count(*)::bigint as count
FROM (
    SELECT unnest(ARRAY[9, 8, 7, 6]) as bucket
) buckets
JOIN matched ON matched.rating_average >= buckets.bucket
GROUP BY bucket
ORDER BY bucket DESC
"#;

pub fn validate_search_query(query: SearchQuery) -> Result<SearchParams, SearchError> {
    let q = query.q.unwrap_or_default().trim().to_string();
    if q.len() > MAX_QUERY_LEN {
        return Err(SearchError::QueryTooLong);
    }

    if let Some(ref media_type) = query.media_type {
        validate_media_type(media_type)
            .map_err(|_| SearchError::InvalidMediaType(media_type.clone()))?;
    }

    if let Some(year) = query.year
        && !(1800..=2100).contains(&year)
    {
        return Err(SearchError::InvalidYear(year));
    }

    if let Some(rating) = query.rating_min
        && !(0.0..=10.0).contains(&rating)
    {
        return Err(SearchError::InvalidRating(rating));
    }

    Ok(SearchParams {
        query: q,
        media_type: query.media_type.filter(|v| !v.trim().is_empty()),
        genre: query.genre.filter(|v| !v.trim().is_empty()),
        year: query.year,
        rating_min: query.rating_min,
        limit: query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
    })
}

pub async fn search_media(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    params: SearchParams,
) -> Result<SearchResponse, SearchError> {
    let started = Instant::now();
    let has_filters = has_active_filters(&params);
    let result = search_media_inner(pool, user, params).await;
    record_search_metrics(started, result.is_ok(), has_filters);
    result
}

async fn search_media_inner(
    pool: &sqlx::PgPool,
    user: &AuthenticatedUser,
    params: SearchParams,
) -> Result<SearchResponse, SearchError> {
    if params.query.is_empty() {
        return Ok(SearchResponse {
            items: Vec::new(),
            facets: SearchFacets::default(),
        });
    }

    let rows = bind_search_query(sqlx::query(SEARCH_ITEMS_SQL), &params, user)
        .bind(params.limit as i64)
        .fetch_all(pool)
        .await?;

    let items = rows.iter().map(row_to_response).collect();

    let (types, genres, years, ratings) = tokio::try_join!(
        run_facet_query(pool, TYPE_FACETS_SQL, &params, user),
        run_facet_query(pool, GENRE_FACETS_SQL, &params, user),
        run_facet_query(pool, YEAR_FACETS_SQL, &params, user),
        run_facet_query(pool, RATING_FACETS_SQL, &params, user),
    )?;

    Ok(SearchResponse {
        items,
        facets: SearchFacets {
            types,
            genres,
            years,
            ratings,
        },
    })
}

fn has_active_filters(params: &SearchParams) -> bool {
    params.media_type.is_some()
        || params.genre.is_some()
        || params.year.is_some()
        || params.rating_min.is_some()
}

fn record_search_metrics(started: Instant, success: bool, has_filters: bool) {
    let status = if success { "success" } else { "error" };
    metrics::counter!(
        "search_queries_total",
        "status" => status,
        "has_filters" => has_filters.to_string()
    )
    .increment(1);
    metrics::histogram!(
        "search_query_duration_seconds",
        "status" => status,
        "has_filters" => has_filters.to_string()
    )
    .record(started.elapsed().as_secs_f64());
}

async fn run_facet_query(
    pool: &sqlx::PgPool,
    sql: &'static str,
    params: &SearchParams,
    user: &AuthenticatedUser,
) -> Result<Vec<SearchFacetCount>, SearchError> {
    let rows = bind_search_query(sqlx::query(sql), params, user)
        .fetch_all(pool)
        .await?;

    Ok(rows
        .iter()
        .map(|row| SearchFacetCount {
            value: row.get("value"),
            label: row.get("label"),
            count: row.get("count"),
        })
        .collect())
}

fn bind_search_query<'q>(
    query: sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    params: &'q SearchParams,
    user: &'q AuthenticatedUser,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    query
        .bind(&params.query)
        .bind(params.media_type.as_deref())
        .bind(params.genre.as_deref())
        .bind(params.year)
        .bind(params.rating_min)
        .bind(user.has_all_library_access)
        .bind(user.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(q: &str) -> SearchQuery {
        SearchQuery {
            q: Some(q.to_string()),
            media_type: None,
            genre: None,
            year: None,
            rating_min: None,
            limit: None,
        }
    }

    #[test]
    fn trims_empty_query_to_empty_params() {
        let params = validate_search_query(query("   ")).unwrap();
        assert!(params.query.is_empty());
    }

    #[test]
    fn rejects_invalid_type() {
        let mut search = query("matrix");
        search.media_type = Some("album".to_string());

        assert!(matches!(
            validate_search_query(search),
            Err(SearchError::InvalidMediaType(_))
        ));
    }

    #[test]
    fn rejects_invalid_year() {
        let mut search = query("matrix");
        search.year = Some(1700);

        assert!(matches!(
            validate_search_query(search),
            Err(SearchError::InvalidYear(1700))
        ));
    }

    #[test]
    fn clamps_limit_to_max() {
        let mut search = query("matrix");
        search.limit = Some(500);

        let params = validate_search_query(search).unwrap();
        assert_eq!(params.limit, MAX_LIMIT);
    }
}
