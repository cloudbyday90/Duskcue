use crate::domains::media::projection::{
    BROWSE_SELECT_SQL, browse_row_to_response, push_browse_children,
};
use crate::domains::profiles::types::ProfileScope;
use crate::error::AppError;

use super::types::{SearchFacets, SearchParams, SearchResponse};
use super::{cursor, facets, query};

pub(super) async fn search(
    pool: &sqlx::PgPool,
    scope: &ProfileScope,
    params: &SearchParams,
) -> Result<SearchResponse, AppError> {
    let context = cursor::context(scope, params);
    let cursor = cursor::decode(params.cursor.as_deref(), &context, &params.sort)?;
    if params.query.is_empty() {
        return Ok(SearchResponse {
            items: vec![],
            facets: SearchFacets::default(),
            cursor: None,
            has_more: false,
        });
    }
    let mut sql = query::matching_query(scope, params);
    sql.push(BROWSE_SELECT_SQL.replace("FROM allowed_media mi", "FROM matched mi"))
        .push_bind(scope.profile_id)
        .push(" AND uid.user_id = ")
        .push_bind(scope.owner_user_id);
    push_browse_children(&mut sql, scope);
    if let Some(cursor) = &cursor {
        cursor::push_page(&mut sql, cursor, &params.order);
    }
    let direction = if params.order == "asc" {
        " ASC"
    } else {
        " DESC"
    };
    match params.sort.as_str() {
        "title" => {
            sql.push(" ORDER BY mi.search_title")
                .push(direction)
                .push(", mi.id")
                .push(direction);
        }
        "year" => {
            sql.push(" ORDER BY mi.search_year")
                .push(direction)
                .push(" NULLS LAST, mi.id")
                .push(direction);
        }
        _ => {
            sql.push(" ORDER BY mi.search_rank")
                .push(direction)
                .push(", mi.search_title ASC, mi.id ASC");
        }
    }
    sql.push(" LIMIT ").push_bind(i64::from(params.limit) + 1);
    let rows = sql
        .build()
        .fetch_all(pool)
        .await
        .map_err(super::SearchError::from)?;
    let has_more = rows.len() > params.limit as usize;
    let page = &rows[..rows.len().min(params.limit as usize)];
    let next_cursor = if has_more {
        page.last()
            .map(|row| cursor::encode(&context, row, &params.sort))
    } else {
        None
    };
    let facets = facets::load(pool, scope, params)
        .await
        .map_err(super::SearchError::from)?;
    Ok(SearchResponse {
        items: page.iter().map(browse_row_to_response).collect(),
        facets,
        cursor: next_cursor,
        has_more,
    })
}
