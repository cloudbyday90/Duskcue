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
use sqlx::Row;
use uuid::Uuid;

use super::error::MediaError;
use super::types::{
    MediaFileResponse, MediaFileRow, MediaItemListResponse, MediaItemResponse, MediaItemRow,
    VALID_IDENTIFICATION_SOURCES, VALID_MATCH_STATES, VALID_MEDIA_ITEM_TYPES,
};

const GET_MEDIA_ITEM_SQL: &str = r#"SELECT mi.*, s.status as series_status,
                  sn.series_id, sn.season_number, sn.id as season_id,
                  ep.episode_number, ep.absolute_episode_number,
                  COALESCE(mf.cnt, 0) as file_count
           FROM media_items mi
           LEFT JOIN series s ON s.id = mi.id
           LEFT JOIN seasons sn ON sn.id = mi.id
           LEFT JOIN episodes ep ON ep.id = mi.id
           LEFT JOIN LATERAL (
               SELECT count(*) as cnt FROM media_files mf WHERE mf.media_item_id = mi.id
           ) mf ON true
           WHERE mi.id = $1"#;

const LIST_MEDIA_ITEMS_DESC_SQL: &str = r#"SELECT mi.*, s.status as series_status,
                  sn.series_id, sn.season_number, sn.id as season_id,
                  ep.episode_number, ep.absolute_episode_number,
                  COALESCE(mf.cnt, 0) as file_count
           FROM media_items mi
           LEFT JOIN series s ON s.id = mi.id
           LEFT JOIN seasons sn ON sn.id = mi.id
           LEFT JOIN episodes ep ON ep.id = mi.id
           LEFT JOIN LATERAL (
               SELECT count(*) as cnt FROM media_files mf WHERE mf.media_item_id = mi.id
           ) mf ON true
           WHERE ($1::uuid IS NULL OR mi.library_id = $1)
             AND ($2::text IS NULL OR mi.type = $2)
             AND ($3::uuid IS NULL OR mi.id < $3)
           ORDER BY mi.id DESC
           LIMIT $4"#;

const LIST_MEDIA_ITEMS_ASC_SQL: &str = r#"SELECT mi.*, s.status as series_status,
                  sn.series_id, sn.season_number, sn.id as season_id,
                  ep.episode_number, ep.absolute_episode_number,
                  COALESCE(mf.cnt, 0) as file_count
           FROM media_items mi
           LEFT JOIN series s ON s.id = mi.id
           LEFT JOIN seasons sn ON sn.id = mi.id
           LEFT JOIN episodes ep ON ep.id = mi.id
           LEFT JOIN LATERAL (
               SELECT count(*) as cnt FROM media_files mf WHERE mf.media_item_id = mi.id
           ) mf ON true
           WHERE ($1::uuid IS NULL OR mi.library_id = $1)
             AND ($2::text IS NULL OR mi.type = $2)
             AND ($3::uuid IS NULL OR mi.id > $3)
           ORDER BY mi.id ASC
           LIMIT $4"#;

pub async fn list_media_items(
    pool: &sqlx::PgPool,
    library_id: Option<Uuid>,
    type_filter: Option<&str>,
    limit: u32,
    cursor: Option<&str>,
    order: &str,
) -> Result<MediaItemListResponse, MediaError> {
    let cursor_id = parse_cursor(cursor);
    let fetch_limit = (limit + 1) as i64;
    let is_asc = order == "asc";

    let sql = if is_asc {
        LIST_MEDIA_ITEMS_ASC_SQL
    } else {
        LIST_MEDIA_ITEMS_DESC_SQL
    };

    let rows = sqlx::query(sql)
        .bind(library_id)
        .bind(type_filter)
        .bind(cursor_id)
        .bind(fetch_limit)
        .fetch_all(pool)
        .await?;

    let has_more = rows.len() > limit as usize;
    let rows = if has_more {
        &rows[..limit as usize]
    } else {
        &rows
    };

    let items: Vec<MediaItemResponse> = rows.iter().map(row_to_response).collect();

    let next_cursor = if has_more {
        items.last().map(|i| encode_cursor(i.id))
    } else {
        None
    };

    Ok(MediaItemListResponse {
        items,
        cursor: next_cursor,
        has_more,
    })
}

pub async fn list_library_items(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    type_filter: Option<&str>,
    limit: u32,
    cursor: Option<&str>,
    order: &str,
) -> Result<MediaItemListResponse, MediaError> {
    verify_library_exists(pool, library_id).await?;
    list_media_items(pool, Some(library_id), type_filter, limit, cursor, order).await
}

pub async fn get_media_item(
    pool: &sqlx::PgPool,
    item_id: Uuid,
) -> Result<MediaItemResponse, MediaError> {
    let row = sqlx::query(GET_MEDIA_ITEM_SQL)
        .bind(item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(MediaError::NotFound)?;

    Ok(row_to_response(&row))
}

pub struct UpdateMediaItemParams {
    pub item_id: Uuid,
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub premiere_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
    pub content_rating: Option<String>,
    pub runtime_seconds: Option<i32>,
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub trakt_id: Option<i64>,
    pub rating_average: Option<f32>,
    pub rating_vote_count: Option<i32>,
    pub metadata: Option<serde_json::Value>,
    pub match_state: Option<String>,
    pub identification_source: Option<String>,
}

pub async fn update_media_item(
    pool: &sqlx::PgPool,
    params: UpdateMediaItemParams,
) -> Result<MediaItemResponse, MediaError> {
    sqlx::query("SELECT id FROM media_items WHERE id = $1")
        .bind(params.item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(MediaError::NotFound)?;

    if let Some(ref ms) = params.match_state
        && !VALID_MATCH_STATES.contains(&ms.as_str())
    {
        return Err(MediaError::InvalidMatchState(ms.clone()));
    }

    if let Some(ref src) = params.identification_source
        && !VALID_IDENTIFICATION_SOURCES.contains(&src.as_str())
    {
        return Err(MediaError::InvalidIdentificationSource(src.clone()));
    }

    let row = sqlx::query(
        r#"UPDATE media_items SET
            title = COALESCE($2, title),
            sort_title = COALESCE($3, sort_title),
            original_title = COALESCE($4, original_title),
            overview = COALESCE($5, overview),
            premiere_date = COALESCE($6, premiere_date),
            end_date = COALESCE($7, end_date),
            content_rating = COALESCE($8, content_rating),
            runtime_seconds = COALESCE($9, runtime_seconds),
            tmdb_id = COALESCE($10, tmdb_id),
            imdb_id = COALESCE($11, imdb_id),
            tvdb_id = COALESCE($12, tvdb_id),
            trakt_id = COALESCE($13, trakt_id),
            rating_average = COALESCE($14, rating_average),
            rating_vote_count = COALESCE($15, rating_vote_count),
            metadata = COALESCE($16, metadata),
            match_state = COALESCE($17, match_state),
            identification_source = COALESCE($18, identification_source),
            updated_at = now()
        WHERE id = $1
        RETURNING id, created_at, updated_at, library_id, type, title, sort_title,
                  original_title, overview, premiere_date, end_date, content_rating,
                  runtime_seconds, tmdb_id, imdb_id, tvdb_id, trakt_id,
                  rating_average, rating_vote_count, metadata, match_state,
                  identification_source"#,
    )
    .bind(params.item_id)
    .bind(&params.title)
    .bind(&params.sort_title)
    .bind(&params.original_title)
    .bind(&params.overview)
    .bind(params.premiere_date)
    .bind(params.end_date)
    .bind(&params.content_rating)
    .bind(params.runtime_seconds)
    .bind(params.tmdb_id)
    .bind(&params.imdb_id)
    .bind(params.tvdb_id)
    .bind(params.trakt_id)
    .bind(params.rating_average)
    .bind(params.rating_vote_count)
    .bind(&params.metadata)
    .bind(&params.match_state)
    .bind(&params.identification_source)
    .fetch_one(pool)
    .await?;

    let item = row_to_base_row(&row);

    let series_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM series WHERE id = $1")
            .bind(item.id)
            .fetch_optional(pool)
            .await?
            .flatten();

    let season_row: Option<(Option<Uuid>, Option<i32>, Option<Uuid>)> =
        sqlx::query_as("SELECT series_id, season_number, id FROM seasons WHERE id = $1")
            .bind(item.id)
            .fetch_optional(pool)
            .await?;

    let (series_id, season_number, season_id) = season_row
        .map(|r| (r.0, r.1, r.2))
        .unwrap_or((None, None, None));

    let episode_row: Option<(Option<i32>, Option<i32>)> = sqlx::query_as(
        "SELECT episode_number, absolute_episode_number FROM episodes WHERE id = $1",
    )
    .bind(item.id)
    .fetch_optional(pool)
    .await?;

    let (episode_number, absolute_episode_number) = episode_row.unwrap_or((None, None));

    let file_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media_files WHERE media_item_id = $1")
            .bind(item.id)
            .fetch_one(pool)
            .await?;

    Ok(MediaItemResponse {
        id: item.id,
        created_at: item.created_at,
        updated_at: item.updated_at,
        library_id: item.library_id,
        r#type: item.r#type,
        title: item.title,
        sort_title: item.sort_title,
        original_title: item.original_title,
        overview: item.overview,
        premiere_date: item.premiere_date,
        end_date: item.end_date,
        content_rating: item.content_rating,
        runtime_seconds: item.runtime_seconds,
        tmdb_id: item.tmdb_id,
        imdb_id: item.imdb_id,
        tvdb_id: item.tvdb_id,
        trakt_id: item.trakt_id,
        rating_average: item.rating_average,
        rating_vote_count: item.rating_vote_count,
        metadata: item.metadata,
        match_state: item.match_state,
        identification_source: item.identification_source,
        series_status,
        series_id,
        season_number,
        season_id,
        episode_number,
        absolute_episode_number,
        file_count: Some(file_count),
    })
}

pub async fn delete_media_item(pool: &sqlx::PgPool, item_id: Uuid) -> Result<(), MediaError> {
    let result = sqlx::query("DELETE FROM media_items WHERE id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(MediaError::NotFound);
    }

    Ok(())
}

pub async fn list_media_files(
    pool: &sqlx::PgPool,
    item_id: Uuid,
) -> Result<Vec<MediaFileResponse>, MediaError> {
    verify_media_item_exists(pool, item_id).await?;

    let rows = sqlx::query(
        r#"SELECT id, created_at, updated_at, media_item_id, file_path, file_size,
                  file_hash, file_modified_at, container_format, video_codec,
                  video_resolution, video_bitrate, video_dynamic_range,
                  video_frame_rate, audio_codec, audio_channels, audio_language,
                  audio_bitrate, runtime_seconds, last_scanned_at, is_healthy,
                  additional_streams
           FROM media_files
           WHERE media_item_id = $1
           ORDER BY created_at ASC"#,
    )
    .bind(item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(file_row_to_response).collect())
}

pub async fn get_media_file(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    file_id: Uuid,
) -> Result<MediaFileResponse, MediaError> {
    verify_media_item_exists(pool, item_id).await?;

    let row = sqlx::query(
        r#"SELECT id, created_at, updated_at, media_item_id, file_path, file_size,
                  file_hash, file_modified_at, container_format, video_codec,
                  video_resolution, video_bitrate, video_dynamic_range,
                  video_frame_rate, audio_codec, audio_channels, audio_language,
                  audio_bitrate, runtime_seconds, last_scanned_at, is_healthy,
                  additional_streams
           FROM media_files
           WHERE id = $1 AND media_item_id = $2"#,
    )
    .bind(file_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await?
    .ok_or(MediaError::FileNotFound)?;

    Ok(file_row_to_response(&row))
}

pub fn validate_media_type(media_type: &str) -> Result<(), MediaError> {
    if VALID_MEDIA_ITEM_TYPES.contains(&media_type) {
        Ok(())
    } else {
        Err(MediaError::InvalidMediaType(format!(
            "Invalid media item type: {}. Must be one of: {}",
            media_type,
            VALID_MEDIA_ITEM_TYPES.join(", ")
        )))
    }
}

async fn verify_library_exists(pool: &sqlx::PgPool, library_id: Uuid) -> Result<(), MediaError> {
    sqlx::query("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(MediaError::NotFound)?;
    Ok(())
}

async fn verify_media_item_exists(pool: &sqlx::PgPool, item_id: Uuid) -> Result<(), MediaError> {
    sqlx::query("SELECT id FROM media_items WHERE id = $1")
        .bind(item_id)
        .fetch_optional(pool)
        .await?
        .ok_or(MediaError::NotFound)?;
    Ok(())
}

fn row_to_base_row(row: &sqlx::postgres::PgRow) -> MediaItemRow {
    MediaItemRow {
        id: row.get("id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        library_id: row.get("library_id"),
        r#type: row.get("type"),
        title: row.get("title"),
        sort_title: row.get("sort_title"),
        original_title: row.try_get("original_title").ok(),
        overview: row.try_get("overview").ok(),
        premiere_date: row.try_get("premiere_date").ok(),
        end_date: row.try_get("end_date").ok(),
        content_rating: row.try_get("content_rating").ok(),
        runtime_seconds: row.try_get("runtime_seconds").ok(),
        tmdb_id: row.try_get("tmdb_id").ok(),
        imdb_id: row.try_get("imdb_id").ok(),
        tvdb_id: row.try_get("tvdb_id").ok(),
        trakt_id: row.try_get("trakt_id").ok(),
        rating_average: row.try_get("rating_average").ok(),
        rating_vote_count: row.try_get("rating_vote_count").ok(),
        metadata: row.get("metadata"),
        match_state: row.get("match_state"),
        identification_source: row.try_get("identification_source").ok(),
    }
}

pub fn row_to_response(row: &sqlx::postgres::PgRow) -> MediaItemResponse {
    let item = row_to_base_row(row);

    let series_status: Option<String> = row.try_get("series_status").ok().flatten();
    let series_id: Option<Uuid> = row.try_get("series_id").ok().flatten();
    let season_number: Option<i32> = row.try_get("season_number").ok().flatten();
    let season_id: Option<Uuid> = row.try_get("season_id").ok().flatten();
    let episode_number: Option<i32> = row.try_get("episode_number").ok().flatten();
    let absolute_episode_number: Option<i32> =
        row.try_get("absolute_episode_number").ok().flatten();
    let file_count: i64 = row.try_get("file_count").unwrap_or(0);

    MediaItemResponse {
        id: item.id,
        created_at: item.created_at,
        updated_at: item.updated_at,
        library_id: item.library_id,
        r#type: item.r#type,
        title: item.title,
        sort_title: item.sort_title,
        original_title: item.original_title,
        overview: item.overview,
        premiere_date: item.premiere_date,
        end_date: item.end_date,
        content_rating: item.content_rating,
        runtime_seconds: item.runtime_seconds,
        tmdb_id: item.tmdb_id,
        imdb_id: item.imdb_id,
        tvdb_id: item.tvdb_id,
        trakt_id: item.trakt_id,
        rating_average: item.rating_average,
        rating_vote_count: item.rating_vote_count,
        metadata: item.metadata,
        match_state: item.match_state,
        identification_source: item.identification_source,
        series_status,
        series_id,
        season_number,
        season_id,
        episode_number,
        absolute_episode_number,
        file_count: if file_count > 0 {
            Some(file_count)
        } else {
            None
        },
    }
}

fn file_row_to_response(row: &sqlx::postgres::PgRow) -> MediaFileResponse {
    let file_row = MediaFileRow {
        id: row.get("id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        media_item_id: row.get("media_item_id"),
        file_path: row.get("file_path"),
        file_size: row.get("file_size"),
        file_hash: row.try_get("file_hash").ok(),
        file_modified_at: row.try_get("file_modified_at").ok(),
        container_format: row.get("container_format"),
        video_codec: row.try_get("video_codec").ok(),
        video_resolution: row.try_get("video_resolution").ok(),
        video_bitrate: row.try_get("video_bitrate").ok(),
        video_dynamic_range: row.try_get("video_dynamic_range").ok(),
        video_frame_rate: row.try_get("video_frame_rate").ok(),
        audio_codec: row.try_get("audio_codec").ok(),
        audio_channels: row.try_get("audio_channels").ok(),
        audio_language: row.try_get("audio_language").ok(),
        audio_bitrate: row.try_get("audio_bitrate").ok(),
        runtime_seconds: row.get("runtime_seconds"),
        last_scanned_at: row.get("last_scanned_at"),
        is_healthy: row.get("is_healthy"),
        additional_streams: row.get("additional_streams"),
    };

    MediaFileResponse {
        id: file_row.id,
        media_item_id: file_row.media_item_id,
        file_path: file_row.file_path,
        file_size: file_row.file_size,
        file_hash: file_row.file_hash,
        file_modified_at: file_row.file_modified_at,
        container_format: file_row.container_format,
        video_codec: file_row.video_codec,
        video_resolution: file_row.video_resolution,
        video_bitrate: file_row.video_bitrate,
        video_dynamic_range: file_row.video_dynamic_range,
        video_frame_rate: file_row.video_frame_rate,
        audio_codec: file_row.audio_codec,
        audio_channels: file_row.audio_channels,
        audio_language: file_row.audio_language,
        audio_bitrate: file_row.audio_bitrate,
        runtime_seconds: file_row.runtime_seconds,
        last_scanned_at: file_row.last_scanned_at,
        is_healthy: file_row.is_healthy,
        additional_streams: file_row.additional_streams,
        created_at: file_row.created_at,
        updated_at: file_row.updated_at,
    }
}

fn parse_cursor(cursor: Option<&str>) -> Option<Uuid> {
    cursor.and_then(|c| {
        let bytes = base64::engine::general_purpose::STANDARD.decode(c).ok()?;
        let json: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        json.get("id")?
            .as_str()
            .and_then(|s| s.parse::<Uuid>().ok())
    })
}

fn encode_cursor(id: Uuid) -> String {
    let json = serde_json::json!({ "id": id.to_string() });
    base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&json).unwrap_or_default())
}
