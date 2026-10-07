use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::media::access::scoped_libraries_query;
use crate::domains::media::types::MediaBrowsePageQuery;
use crate::domains::profiles::types::ProfileScope;
use crate::error::AppError;
use crate::services::browse_cursor;

use super::error::LibrariesError;
use super::types::{BrowseLibraryListResponse, BrowseLibraryResponse};

pub async fn list(
    pool: &PgPool,
    scope: &ProfileScope,
    query: &MediaBrowsePageQuery,
) -> Result<BrowseLibraryListResponse, AppError> {
    let limit = browse_cursor::limit(query.limit)?;
    let context =
        serde_json::json!([scope.owner_user_id, scope.profile_id, "libraries"]).to_string();
    let cursor = browse_cursor::decode::<String>(query.cursor.as_deref(), &context)?;
    let mut sql = scoped_libraries_query(scope);
    sql.push("SELECT id, name, media_type FROM allowed_libraries WHERE true");
    if let Some(cursor) = cursor {
        sql.push(" AND (name, id) > (")
            .push_bind(cursor.key)
            .push(", ")
            .push_bind(cursor.id)
            .push(")");
    }
    sql.push(" ORDER BY name, id LIMIT ")
        .push_bind(i64::from(limit) + 1);
    let rows = sql
        .build()
        .fetch_all(pool)
        .await
        .map_err(LibrariesError::from)?;
    let has_more = rows.len() > limit as usize;
    let items: Vec<_> = rows
        .iter()
        .take(limit as usize)
        .map(row_to_response)
        .collect();
    let cursor = if has_more {
        items
            .last()
            .map(|item| browse_cursor::encode(&context, item.id, item.name.clone()))
    } else {
        None
    };
    Ok(BrowseLibraryListResponse {
        items,
        cursor,
        has_more,
    })
}

pub async fn get(
    pool: &PgPool,
    scope: &ProfileScope,
    id: Uuid,
) -> Result<BrowseLibraryResponse, AppError> {
    let mut sql = scoped_libraries_query(scope);
    sql.push("SELECT id, name, media_type FROM allowed_libraries WHERE id = ")
        .push_bind(id);
    let row = sql
        .build()
        .fetch_optional(pool)
        .await
        .map_err(LibrariesError::from)?
        .ok_or(LibrariesError::NotFound)?;
    Ok(row_to_response(&row))
}

fn row_to_response(row: &sqlx::postgres::PgRow) -> BrowseLibraryResponse {
    BrowseLibraryResponse {
        id: row.get("id"),
        name: row.get("name"),
        r#type: row.get("media_type"),
    }
}
