use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::domains::media::MediaError;
use crate::domains::profiles::types::ProfileScope;
use crate::services::browse_cursor::{self, ScopedBrowseCursor};

use super::types::SearchParams;

#[derive(Debug, Serialize, Deserialize)]
pub(super) enum SearchCursorKey {
    Relevance { rank: f32, title: String },
    Title(String),
    Year(Option<i32>),
}

pub(super) fn context(scope: &ProfileScope, params: &SearchParams) -> String {
    serde_json::json!([
        scope.owner_user_id,
        scope.profile_id,
        "search",
        params.query,
        params.media_type,
        params.genre,
        params.year,
        params.rating_min,
        params.sort,
        params.order,
        params.watch,
        params.favorite,
    ])
    .to_string()
}

pub(super) fn decode(
    value: Option<&str>,
    context: &str,
    sort: &str,
) -> Result<Option<ScopedBrowseCursor<SearchCursorKey>>, MediaError> {
    let cursor = browse_cursor::decode::<SearchCursorKey>(value, context)?;
    if let Some(cursor) = &cursor {
        let valid = match (&cursor.key, sort) {
            (SearchCursorKey::Relevance { rank, .. }, "relevance") => {
                rank.is_finite() && *rank >= 0.0
            }
            (SearchCursorKey::Title(_), "title") => true,
            (SearchCursorKey::Year(year), "year") => {
                year.is_none_or(|year| (1..=9999).contains(&year))
            }
            _ => false,
        };
        if !valid {
            return Err(MediaError::InvalidBrowseCursor);
        }
    }
    Ok(cursor)
}

pub(super) fn push_page(
    sql: &mut QueryBuilder<Postgres>,
    cursor: &ScopedBrowseCursor<SearchCursorKey>,
    order: &str,
) {
    let relation = if order == "asc" { " > " } else { " < " };
    match &cursor.key {
        SearchCursorKey::Relevance { rank, title } => {
            sql.push(" AND (mi.search_rank ")
                .push(relation)
                .push_bind(*rank)
                .push(" OR (mi.search_rank = ")
                .push_bind(*rank)
                .push(" AND (mi.search_title, mi.id) > (")
                .push_bind(title.clone())
                .push(", ")
                .push_bind(cursor.id)
                .push(")))");
        }
        SearchCursorKey::Title(title) => {
            sql.push(" AND (mi.search_title, mi.id)")
                .push(relation)
                .push("(")
                .push_bind(title.clone())
                .push(", ")
                .push_bind(cursor.id)
                .push(")");
        }
        SearchCursorKey::Year(Some(year)) => {
            sql.push(" AND ((mi.search_year, mi.id)")
                .push(relation)
                .push("(")
                .push_bind(*year)
                .push(", ")
                .push_bind(cursor.id)
                .push(") OR mi.search_year IS NULL)");
        }
        SearchCursorKey::Year(None) => {
            sql.push(" AND mi.search_year IS NULL AND mi.id")
                .push(relation)
                .push_bind(cursor.id);
        }
    }
}

pub(super) fn encode(context: &str, row: &sqlx::postgres::PgRow, sort: &str) -> String {
    let key = match sort {
        "title" => SearchCursorKey::Title(row.get("search_title")),
        "year" => SearchCursorKey::Year(row.get("search_year")),
        _ => SearchCursorKey::Relevance {
            rank: row.get("search_rank"),
            title: row.get("search_title"),
        },
    };
    browse_cursor::encode(context, row.get::<Uuid, _>("id"), key)
}
