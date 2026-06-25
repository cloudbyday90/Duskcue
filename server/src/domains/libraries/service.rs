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
use uuid::Uuid;

use super::error::LibrariesError;
use super::types::{
    LibraryListResponse, LibraryPathResponse, LibraryPathRow, LibraryResponse, LibraryRow,
    VALID_MEDIA_TYPES,
};

pub async fn list_libraries(
    pool: &sqlx::PgPool,
    page: u32,
    page_size: u32,
    media_type_filter: Option<&str>,
) -> Result<LibraryListResponse, LibrariesError> {
    let offset = (page - 1) * page_size;

    let count_row = sqlx::query(
        r#"SELECT count(*) as cnt FROM libraries
           WHERE deleted_at IS NULL
             AND ($1::text IS NULL OR media_type = $1)"#,
    )
    .bind(media_type_filter)
    .fetch_one(pool)
    .await?;

    let total: i64 = count_row.get("cnt");

    let rows = sqlx::query(
        r#"SELECT l.id, l.name, l.slug, l.media_type, l.root_path,
                  l.scan_enabled, l.scan_interval_seconds, l.metadata_language,
                  l.metadata, l.last_scan_at, l.deleted_at, l.created_at, l.updated_at,
                  COALESCE(mi.cnt, 0) as item_count
           FROM libraries l
           LEFT JOIN LATERAL (
               SELECT count(*) as cnt FROM media_items mi WHERE mi.library_id = l.id
           ) mi ON true
           WHERE l.deleted_at IS NULL
             AND ($1::text IS NULL OR l.media_type = $1)
           ORDER BY l.created_at DESC
           LIMIT $2 OFFSET $3"#,
    )
    .bind(media_type_filter)
    .bind(page_size as i64)
    .bind(offset as i64)
    .fetch_all(pool)
    .await?;

    let items: Vec<LibraryResponse> = rows.iter().map(row_to_response).collect();

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (page_size as f64)).ceil() as u32
    };

    Ok(LibraryListResponse {
        items,
        total,
        page,
        page_size,
        total_pages,
    })
}

pub async fn get_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<LibraryResponse, LibrariesError> {
    let row = sqlx::query(
        r#"SELECT l.id, l.name, l.slug, l.media_type, l.root_path,
                  l.scan_enabled, l.scan_interval_seconds, l.metadata_language,
                  l.metadata, l.last_scan_at, l.deleted_at, l.created_at, l.updated_at,
                  COALESCE(mi.cnt, 0) as item_count
           FROM libraries l
           LEFT JOIN LATERAL (
               SELECT count(*) as cnt FROM media_items mi WHERE mi.library_id = l.id
           ) mi ON true
           WHERE l.id = $1 AND l.deleted_at IS NULL"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?
    .ok_or(LibrariesError::NotFound)?;

    Ok(row_to_response(&row))
}

pub struct CreateLibraryParams {
    pub name: String,
    pub slug: String,
    pub media_type: String,
    pub root_path: String,
    pub scan_interval_seconds: i32,
    pub metadata_language: String,
}

pub async fn create_library(
    pool: &sqlx::PgPool,
    params: CreateLibraryParams,
) -> Result<LibraryResponse, LibrariesError> {
    let existing = sqlx::query("SELECT id FROM libraries WHERE slug = $1 AND deleted_at IS NULL")
        .bind(&params.slug)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return Err(LibrariesError::NameExists(params.name));
    }

    let existing_name =
        sqlx::query("SELECT id FROM libraries WHERE name = $1 AND deleted_at IS NULL")
            .bind(&params.name)
            .fetch_optional(pool)
            .await?;

    if existing_name.is_some() {
        return Err(LibrariesError::NameExists(params.name));
    }

    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        r#"INSERT INTO libraries (name, slug, media_type, root_path, scan_interval_seconds, metadata_language)
           VALUES ($1, $2, $3, $4, $5, $6)
           RETURNING id, name, slug, media_type, root_path, scan_enabled,
                     scan_interval_seconds, metadata_language, metadata,
                     last_scan_at, deleted_at, created_at, updated_at"#,
    )
    .bind(&params.name)
    .bind(&params.slug)
    .bind(&params.media_type)
    .bind(&params.root_path)
    .bind(params.scan_interval_seconds)
    .bind(&params.metadata_language)
    .fetch_one(&mut *tx)
    .await?;

    let library_id: Uuid = row.get("id");

    sqlx::query(
        "INSERT INTO library_paths (library_id, path, is_default, scan_enabled) VALUES ($1, $2, true, true)",
    )
    .bind(library_id)
    .bind(&params.root_path)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let mut response = row_to_response(&row);
    response.item_count = 0;
    Ok(response)
}

pub struct UpdateLibraryParams {
    pub library_id: Uuid,
    pub name: Option<String>,
    pub slug: Option<String>,
    pub root_path: Option<String>,
    pub scan_enabled: Option<bool>,
    pub scan_interval_seconds: Option<i32>,
    pub metadata_language: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

pub async fn update_library(
    pool: &sqlx::PgPool,
    params: UpdateLibraryParams,
) -> Result<LibraryResponse, LibrariesError> {
    sqlx::query("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
        .bind(params.library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(LibrariesError::NotFound)?;

    if let Some(ref name) = params.name {
        let existing = sqlx::query(
            "SELECT id FROM libraries WHERE name = $1 AND id != $2 AND deleted_at IS NULL",
        )
        .bind(name)
        .bind(params.library_id)
        .fetch_optional(pool)
        .await?;

        if existing.is_some() {
            return Err(LibrariesError::NameExists(name.clone()));
        }
    }

    if let Some(ref slug) = params.slug {
        let existing = sqlx::query(
            "SELECT id FROM libraries WHERE slug = $1 AND id != $2 AND deleted_at IS NULL",
        )
        .bind(slug)
        .bind(params.library_id)
        .fetch_optional(pool)
        .await?;

        if existing.is_some() {
            let display_name = params.name.as_deref().unwrap_or(slug);
            return Err(LibrariesError::NameExists(display_name.to_string()));
        }
    }

    let row = sqlx::query(
        r#"UPDATE libraries SET
            name = COALESCE($2, name),
            slug = COALESCE($3, slug),
            root_path = COALESCE($4, root_path),
            scan_enabled = COALESCE($5, scan_enabled),
            scan_interval_seconds = COALESCE($6, scan_interval_seconds),
            metadata_language = COALESCE($7, metadata_language),
            metadata = COALESCE($8, metadata),
            updated_at = now()
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING id, name, slug, media_type, root_path, scan_enabled,
                  scan_interval_seconds, metadata_language, metadata,
                  last_scan_at, deleted_at, created_at, updated_at"#,
    )
    .bind(params.library_id)
    .bind(&params.name)
    .bind(&params.slug)
    .bind(&params.root_path)
    .bind(params.scan_enabled)
    .bind(params.scan_interval_seconds)
    .bind(&params.metadata_language)
    .bind(&params.metadata)
    .fetch_optional(pool)
    .await?
    .ok_or(LibrariesError::NotFound)?;

    let library_id: Uuid = row.get("id");

    let item_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media_items WHERE library_id = $1")
            .bind(library_id)
            .fetch_one(pool)
            .await?;

    let mut response = row_to_response(&row);
    response.item_count = item_count;
    Ok(response)
}

pub async fn soft_delete_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<(), LibrariesError> {
    sqlx::query("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(LibrariesError::NotFound)?;

    let item_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM media_items WHERE library_id = $1")
            .bind(library_id)
            .fetch_one(pool)
            .await?;

    if item_count > 0 {
        return Err(LibrariesError::CannotDeleteWithMedia);
    }

    sqlx::query(
        "UPDATE libraries SET deleted_at = now(), updated_at = now(), scan_enabled = false WHERE id = $1",
    )
    .bind(library_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub fn generate_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn validate_media_type(media_type: &str) -> Result<(), LibrariesError> {
    if VALID_MEDIA_TYPES.contains(&media_type) {
        Ok(())
    } else {
        Err(LibrariesError::ProviderIdTagMalformed(format!(
            "Invalid media_type: {}. Must be one of: {}",
            media_type,
            VALID_MEDIA_TYPES.join(", ")
        )))
    }
}

pub fn row_to_library_row(row: &sqlx::postgres::PgRow) -> LibraryRow {
    LibraryRow {
        id: row.get("id"),
        name: row.get("name"),
        slug: row.get("slug"),
        media_type: row.get("media_type"),
        root_path: row.get("root_path"),
        scan_enabled: row.get("scan_enabled"),
        scan_interval_seconds: row.get("scan_interval_seconds"),
        metadata_language: row.get("metadata_language"),
        metadata: row.get("metadata"),
        last_scan_at: row.try_get("last_scan_at").ok(),
        deleted_at: row.try_get("deleted_at").ok(),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn row_to_response(row: &sqlx::postgres::PgRow) -> LibraryResponse {
    let lib = row_to_library_row(row);
    let item_count: i64 = row.try_get("item_count").unwrap_or(0);
    LibraryResponse {
        id: lib.id,
        name: lib.name,
        slug: lib.slug,
        media_type: lib.media_type,
        root_path: lib.root_path,
        scan_enabled: lib.scan_enabled,
        scan_interval_seconds: lib.scan_interval_seconds,
        metadata_language: lib.metadata_language,
        metadata: lib.metadata,
        last_scan_at: lib.last_scan_at,
        item_count,
        created_at: lib.created_at,
        updated_at: lib.updated_at,
    }
}

fn path_row_to_response(row: &sqlx::postgres::PgRow) -> LibraryPathResponse {
    let path_row = LibraryPathRow {
        id: row.get("id"),
        created_at: row.get("created_at"),
        library_id: row.get("library_id"),
        path: row.get("path"),
        is_default: row.get("is_default"),
        scan_enabled: row.get("scan_enabled"),
        last_scan_at: row.try_get("last_scan_at").ok(),
    };
    LibraryPathResponse {
        id: path_row.id,
        library_id: path_row.library_id,
        path: path_row.path,
        is_default: path_row.is_default,
        scan_enabled: path_row.scan_enabled,
        last_scan_at: path_row.last_scan_at,
        created_at: path_row.created_at,
    }
}

async fn verify_library_exists(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<(), LibrariesError> {
    sqlx::query("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
        .bind(library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(LibrariesError::NotFound)?;
    Ok(())
}

pub async fn list_library_paths(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<Vec<LibraryPathResponse>, LibrariesError> {
    verify_library_exists(pool, library_id).await?;

    let rows = sqlx::query(
        r#"SELECT id, created_at, library_id, path, is_default, scan_enabled, last_scan_at
           FROM library_paths
           WHERE library_id = $1
           ORDER BY is_default DESC, created_at ASC"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(path_row_to_response).collect())
}

pub async fn get_library_path(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    path_id: Uuid,
) -> Result<LibraryPathResponse, LibrariesError> {
    verify_library_exists(pool, library_id).await?;

    let row = sqlx::query(
        r#"SELECT id, created_at, library_id, path, is_default, scan_enabled, last_scan_at
           FROM library_paths
           WHERE id = $1 AND library_id = $2"#,
    )
    .bind(path_id)
    .bind(library_id)
    .fetch_optional(pool)
    .await?
    .ok_or(LibrariesError::PathNotFound)?;

    Ok(path_row_to_response(&row))
}

pub struct CreateLibraryPathParams {
    pub library_id: Uuid,
    pub path: String,
    pub is_default: bool,
    pub scan_enabled: bool,
}

pub async fn create_library_path(
    pool: &sqlx::PgPool,
    params: CreateLibraryPathParams,
) -> Result<LibraryPathResponse, LibrariesError> {
    verify_library_exists(pool, params.library_id).await?;

    let existing = sqlx::query("SELECT id FROM library_paths WHERE library_id = $1 AND path = $2")
        .bind(params.library_id)
        .bind(&params.path)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return Err(LibrariesError::PathExists(params.path));
    }

    let mut tx = pool.begin().await?;

    if params.is_default {
        sqlx::query(
            "UPDATE library_paths SET is_default = false WHERE library_id = $1 AND is_default = true",
        )
        .bind(params.library_id)
        .execute(&mut *tx)
        .await?;
    }

    let row = sqlx::query(
        r#"INSERT INTO library_paths (library_id, path, is_default, scan_enabled)
           VALUES ($1, $2, $3, $4)
           RETURNING id, created_at, library_id, path, is_default, scan_enabled, last_scan_at"#,
    )
    .bind(params.library_id)
    .bind(&params.path)
    .bind(params.is_default)
    .bind(params.scan_enabled)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(path_row_to_response(&row))
}

pub struct UpdateLibraryPathParams {
    pub library_id: Uuid,
    pub path_id: Uuid,
    pub path: Option<String>,
    pub is_default: Option<bool>,
    pub scan_enabled: Option<bool>,
}

pub async fn update_library_path(
    pool: &sqlx::PgPool,
    params: UpdateLibraryPathParams,
) -> Result<LibraryPathResponse, LibrariesError> {
    verify_library_exists(pool, params.library_id).await?;

    sqlx::query("SELECT id FROM library_paths WHERE id = $1 AND library_id = $2")
        .bind(params.path_id)
        .bind(params.library_id)
        .fetch_optional(pool)
        .await?
        .ok_or(LibrariesError::PathNotFound)?;

    if let Some(ref new_path) = params.path {
        let existing = sqlx::query(
            "SELECT id FROM library_paths WHERE library_id = $1 AND path = $2 AND id != $3",
        )
        .bind(params.library_id)
        .bind(new_path)
        .bind(params.path_id)
        .fetch_optional(pool)
        .await?;

        if existing.is_some() {
            return Err(LibrariesError::PathExists(new_path.clone()));
        }
    }

    let mut tx = pool.begin().await?;

    if params.is_default == Some(true) {
        sqlx::query(
            "UPDATE library_paths SET is_default = false WHERE library_id = $1 AND is_default = true",
        )
        .bind(params.library_id)
        .execute(&mut *tx)
        .await?;
    }

    let row = sqlx::query(
        r#"UPDATE library_paths SET
            path = COALESCE($3, path),
            is_default = COALESCE($4, is_default),
            scan_enabled = COALESCE($5, scan_enabled)
        WHERE id = $1 AND library_id = $2
        RETURNING id, created_at, library_id, path, is_default, scan_enabled, last_scan_at"#,
    )
    .bind(params.path_id)
    .bind(params.library_id)
    .bind(&params.path)
    .bind(params.is_default)
    .bind(params.scan_enabled)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(path_row_to_response(&row))
}

pub async fn delete_library_path(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    path_id: Uuid,
) -> Result<(), LibrariesError> {
    verify_library_exists(pool, library_id).await?;

    let row =
        sqlx::query("SELECT id, is_default FROM library_paths WHERE id = $1 AND library_id = $2")
            .bind(path_id)
            .bind(library_id)
            .fetch_optional(pool)
            .await?
            .ok_or(LibrariesError::PathNotFound)?;

    let is_default: bool = row.get("is_default");

    if is_default {
        let path_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM library_paths WHERE library_id = $1")
                .bind(library_id)
                .fetch_one(pool)
                .await?;

        if path_count <= 1 {
            return Err(LibrariesError::CannotDeleteDefaultPath);
        }
    }

    sqlx::query("DELETE FROM library_paths WHERE id = $1 AND library_id = $2")
        .bind(path_id)
        .bind(library_id)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn list_library_path_strings(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<Vec<String>, LibrariesError> {
    let rows =
        sqlx::query("SELECT path FROM library_paths WHERE library_id = $1 AND scan_enabled = true")
            .bind(library_id)
            .fetch_all(pool)
            .await?;

    Ok(rows.iter().map(|r| r.get::<String, _>("path")).collect())
}
