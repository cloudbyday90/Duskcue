use sqlx::{Postgres, QueryBuilder};

use crate::domains::media::access::scoped_media_query;
use crate::domains::profiles::types::ProfileScope;

use super::types::SearchParams;

pub(super) fn matching_query(
    scope: &ProfileScope,
    params: &SearchParams,
) -> QueryBuilder<Postgres> {
    let mut sql = scoped_media_query(scope);
    sql.push(", search_query AS (SELECT plainto_tsquery('english', ")
        .push_bind(params.query.clone())
        .push("::text) AS query), matched AS (SELECT mi.*, ts_rank(mi.search_vector, sq.query) AS search_rank, COALESCE(mi.sort_title, mi.title) AS search_title, EXTRACT(YEAR FROM mi.premiere_date)::int AS search_year FROM allowed_media mi CROSS JOIN search_query sq LEFT JOIN user_item_data uid ON uid.media_item_id = mi.id AND uid.profile_id = ")
        .push_bind(scope.profile_id).push(" AND uid.user_id = ").push_bind(scope.owner_user_id)
        .push(" WHERE mi.search_vector @@ sq.query");
    if let Some(value) = &params.media_type {
        sql.push(" AND mi.type = ").push_bind(value.clone());
    }
    if let Some(value) = &params.genre {
        sql.push(" AND EXISTS (SELECT 1 FROM media_genres mg JOIN genres g ON g.id = mg.genre_id WHERE mg.media_item_id = mi.id AND g.slug = ").push_bind(value.clone()).push(")");
    }
    if let Some(value) = params.year {
        sql.push(" AND EXTRACT(YEAR FROM mi.premiere_date)::int = ")
            .push_bind(value);
    }
    if let Some(value) = params.rating_min {
        sql.push(" AND mi.rating_average >= ").push_bind(value);
    }
    if let Some(value) = params.favorite {
        sql.push(" AND COALESCE(uid.is_favorite, false) = ")
            .push_bind(value);
    }
    match params.watch.as_str() {
        "watched" => {
            sql.push(" AND COALESCE(uid.is_watched, false)");
        }
        "unwatched" => {
            sql.push(" AND NOT COALESCE(uid.is_watched, false)");
        }
        "in_progress" => {
            sql.push(" AND NOT COALESCE(uid.is_watched, false) AND uid.resume_position_ms > 0 AND uid.last_played_at IS NOT NULL");
        }
        _ => {}
    }
    sql.push(") ");
    sql
}
