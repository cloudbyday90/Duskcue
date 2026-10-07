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
use super::error::SearchError;
use super::types::{SearchParams, SearchQuery, SearchResponse};
use crate::domains::media::service::validate_media_type;
use crate::domains::profiles::types::ProfileScope;
use crate::error::AppError;
use std::time::Instant;

const MAX_QUERY_LEN: usize = 200;
const DEFAULT_LIMIT: u32 = 40;
const MAX_LIMIT: u32 = 100;

pub fn validate_search_query(query: SearchQuery) -> Result<SearchParams, SearchError> {
    let q = query.q.unwrap_or_default().trim().to_string();
    if q.chars().count() > MAX_QUERY_LEN {
        return Err(SearchError::QueryTooLong);
    }
    if let Some(value) = &query.media_type {
        validate_media_type(value).map_err(|_| SearchError::InvalidMediaType(value.clone()))?;
    }
    if let Some(value) = query.year
        && !(1800..=2100).contains(&value)
    {
        return Err(SearchError::InvalidYear(value));
    }
    if let Some(value) = query.rating_min
        && !(0.0..=10.0).contains(&value)
    {
        return Err(SearchError::InvalidRating(value));
    }
    let sort = query.sort.as_deref().unwrap_or("relevance");
    if !["relevance", "title", "year"].contains(&sort) {
        return Err(SearchError::InvalidBrowseQuery("unknown sort".into()));
    }
    let order = query
        .order
        .as_deref()
        .unwrap_or(if sort == "title" { "asc" } else { "desc" });
    if !["asc", "desc"].contains(&order) {
        return Err(SearchError::InvalidBrowseQuery("unknown order".into()));
    }
    let watch = query.watch.as_deref().unwrap_or("all");
    if !["all", "watched", "unwatched", "in_progress"].contains(&watch) {
        return Err(SearchError::InvalidBrowseQuery(
            "unknown watch filter".into(),
        ));
    }
    if query.genre.as_ref().is_some_and(|value| value.len() > 128) {
        return Err(SearchError::InvalidBrowseQuery("genre is too long".into()));
    }
    Ok(SearchParams {
        query: q,
        sort: sort.into(),
        order: order.into(),
        watch: watch.into(),
        media_type: query.media_type.filter(|v| !v.trim().is_empty()),
        genre: query.genre.filter(|v| !v.trim().is_empty()),
        year: query.year,
        rating_min: query.rating_min,
        limit: query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        cursor: query.cursor,
        favorite: query.favorite,
    })
}

pub async fn search_media(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    params: SearchParams,
) -> Result<SearchResponse, AppError> {
    let started = Instant::now();
    let has_filters = params.media_type.is_some()
        || params.genre.is_some()
        || params.year.is_some()
        || params.rating_min.is_some()
        || params.favorite.is_some()
        || params.watch != "all";
    let result = super::browsing::search(pool, scope, &params).await;
    let status = if result.is_ok() { "success" } else { "error" };
    metrics::counter!("search_queries_total", "status" => status, "has_filters" => has_filters.to_string()).increment(1);
    metrics::histogram!("search_query_duration_seconds", "status" => status, "has_filters" => has_filters.to_string()).record(started.elapsed().as_secs_f64());
    result
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
            ..Default::default()
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

    #[test]
    fn supports_unicode_queries_by_character_count() {
        assert!(validate_search_query(query(&"é".repeat(200))).is_ok());
        assert!(matches!(
            validate_search_query(query(&"é".repeat(201))),
            Err(SearchError::QueryTooLong)
        ));
    }

    #[test]
    fn rejects_unsupported_browsing_values() {
        for (sort, order, watch) in [
            ("random", "asc", "all"),
            ("title", "random", "all"),
            ("title", "asc", "random"),
        ] {
            let mut search = query("night");
            search.sort = Some(sort.into());
            search.order = Some(order.into());
            search.watch = Some(watch.into());
            assert!(matches!(
                validate_search_query(search),
                Err(SearchError::InvalidBrowseQuery(_))
            ));
        }
    }
}
