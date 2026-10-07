use sqlx::{PgPool, Postgres, QueryBuilder, Row};
use uuid::Uuid;

use crate::domains::media::access::{push_library_scope, scoped_media_query};
use crate::domains::media::types::{
    MediaBrowseListResponse, MediaBrowsePageQuery, MediaBrowseQuery,
};
use crate::domains::profiles::types::ProfileScope;
use crate::error::AppError;
use crate::services::browse_cursor;

use super::error::CollectionsError;
use super::types::{BrowseCollectionListResponse, BrowseCollectionResponse};

fn collection_query(scope: &ProfileScope) -> QueryBuilder<Postgres> {
    let mut sql = scoped_media_query(scope);
    sql.push(", browsable_collections AS (SELECT c.id, c.name, c.description, count(member.id) AS item_count,
        (SELECT cover.media_item_id FROM collection_items cover JOIN allowed_media cover_media ON cover_media.id = cover.media_item_id
         WHERE cover.collection_id = c.id AND cover.is_missing = false ORDER BY cover.position, cover.media_item_id LIMIT 1) AS cover_media_item_id
        FROM collections c JOIN collection_items member ON member.collection_id = c.id AND member.is_missing = false
        JOIN allowed_media mi ON mi.id = member.media_item_id
        WHERE c.is_enabled AND c.visibility IN ('visible', 'featured') AND (c.library_id IS NULL OR (true");
    push_library_scope(&mut sql, scope, "c.library_id");
    sql.push(")) GROUP BY c.id HAVING count(member.id) > 0) ");
    sql
}

pub async fn list(
    pool: &PgPool,
    scope: &ProfileScope,
    query: &MediaBrowsePageQuery,
) -> Result<BrowseCollectionListResponse, AppError> {
    let limit = browse_cursor::limit(query.limit)?;
    let context =
        serde_json::json!([scope.owner_user_id, scope.profile_id, "collections"]).to_string();
    let cursor = browse_cursor::decode::<String>(query.cursor.as_deref(), &context)?;
    let mut sql = collection_query(scope);
    sql.push("SELECT * FROM browsable_collections WHERE true");
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
        .map_err(CollectionsError::from)?;
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
    Ok(BrowseCollectionListResponse {
        items,
        cursor,
        has_more,
    })
}

pub async fn get(
    pool: &PgPool,
    scope: &ProfileScope,
    id: Uuid,
) -> Result<BrowseCollectionResponse, AppError> {
    let mut sql = collection_query(scope);
    sql.push("SELECT * FROM browsable_collections WHERE id = ")
        .push_bind(id);
    let row = sql
        .build()
        .fetch_optional(pool)
        .await
        .map_err(CollectionsError::from)?
        .ok_or(CollectionsError::NotFound)?;
    Ok(row_to_response(&row))
}

pub async fn items(
    pool: &PgPool,
    scope: &ProfileScope,
    id: Uuid,
    query: &MediaBrowseQuery,
) -> Result<MediaBrowseListResponse, AppError> {
    get(pool, scope, id).await?;
    Ok(crate::domains::media::service::browse_collection_items(pool, scope, id, query).await?)
}

fn row_to_response(row: &sqlx::postgres::PgRow) -> BrowseCollectionResponse {
    BrowseCollectionResponse {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        item_count: row.get("item_count"),
        cover_media_item_id: row.get("cover_media_item_id"),
    }
}
