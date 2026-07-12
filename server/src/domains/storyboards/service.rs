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

//! Storyboard domain service — DB access for the `storyboards` table plus
//! file-path resolution for serving the WebVTT index and WebP sprite sheets.
//!
//! The generation pipeline itself (FFmpeg frame extraction, sprite assembly,
//! WebVTT index authoring) lives in
//! [`crate::services::storyboards`](../../services/storyboards/index.html)
//! (Task 4). The background worker that orchestrates per-library generation
//! lives in
//! [`crate::workers::storyboard_generator`](../../workers/storyboard_generator/index.html)
//! (Task 6). The trigger functions here enqueue work on the scheduler
//! (mirroring `subtitle_auto_fetch` from Phase 9 Task 7 and
//! `segment_analysis` from Phase 10 Task 5).
//!
//! Storyboard files are stored under `{data_dir}/cache/storyboards/{media_file_id}/`
//! per STORYBOARDS.md — `data_dir` comes from `BootstrapConfig` via `AppState`.
//!
//! All queries use runtime `sqlx::query` (not compile-time `query!`)
//! consistent with the auth/users/segments/etc. domain convention — no
//! `DATABASE_URL` is required at build time.

use std::path::{Path, PathBuf};

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::storyboards::error::StoryboardError;
use crate::domains::storyboards::types::*;
use crate::services::storyboards as sb_svc;
use crate::state::AppState;

/// Subdirectory under `cache_dir` that holds all storyboard artifacts.
const STORYBOARDS_SUBDIR: &str = "storyboards";

/// Get storyboard metadata for a media item.
///
/// Resolves the requested healthy media file or the primary fallback, loads
/// the matching `storyboards` row, and builds the response with sprite URLs.
///
/// Returns `StoryboardNotFound` (MEDIA_007) when no storyboard has been
/// generated yet for this item, and `MediaItemNotFound` (MEDIA_001) when
/// the item itself does not exist.
pub async fn get_storyboard(
    pool: &PgPool,
    media_item_id: Uuid,
    requested_media_file_id: Option<Uuid>,
) -> Result<StoryboardResponse, StoryboardError> {
    let media_file_id =
        resolve_media_file_for_storyboard(pool, media_item_id, requested_media_file_id).await?;

    let row = sqlx::query(
        "SELECT id, media_file_id, file_hash, interval_seconds, width, height, \
         sprite_count, total_thumbnails, total_size_bytes, keyframe_only, quality, \
         generated_at, generation_duration_ms, metadata \
         FROM storyboards WHERE media_file_id = $1",
    )
    .bind(media_file_id)
    .fetch_optional(pool)
    .await?;

    let row = row.ok_or(StoryboardError::StoryboardNotFound { media_item_id })?;

    let interval_seconds: i32 = row.get("interval_seconds");
    let width: i32 = row.get("width");
    let height: i32 = row.get("height");
    let sprite_count: i32 = row.get("sprite_count");
    let total_thumbnails: i32 = row.get("total_thumbnails");
    let generated_at: chrono::DateTime<chrono::Utc> = row.get("generated_at");

    // Recover the per-generation grid (columns/rows) from metadata. The
    // worker writes `metadata.columns` and `metadata.rows` when it creates
    // the row; fall back to the design defaults (10×20) when absent so
    // a storyboards row written by a future config still serves cleanly.
    let metadata: serde_json::Value = row.get("metadata");
    let (columns, rows) = read_grid_shape(&metadata);

    let thumbnails_per_sheet = (columns * rows).max(1);
    let mut sprites = Vec::with_capacity(sprite_count as usize);
    for sheet_index in 0..sprite_count {
        let thumbnails_in_sheet = thumbnails_in_sheet(
            sheet_index,
            sprite_count,
            thumbnails_per_sheet,
            total_thumbnails,
        );
        sprites.push(SpriteResponse {
            url: sprite_url_for(media_item_id, media_file_id, sheet_index as u32),
            thumbnails: thumbnails_in_sheet,
            columns,
            rows,
        });
    }

    Ok(StoryboardResponse {
        media_file_id,
        interval_seconds,
        width,
        height,
        sprite_count,
        total_thumbnails,
        index_url: index_url_for(media_item_id, media_file_id),
        sprites,
        generated_at,
    })
}

/// Read the WebVTT index file content for a media item's storyboard.
///
/// Returns the raw `index.vtt` text (served with `text/vtt` content type).
/// Returns `StoryboardNotFound` when no storyboard exists yet.
pub async fn get_storyboard_index(
    pool: &PgPool,
    media_item_id: Uuid,
    requested_media_file_id: Option<Uuid>,
    cache_dir: &Path,
) -> Result<String, StoryboardError> {
    let media_file_id =
        resolve_media_file_for_storyboard(pool, media_item_id, requested_media_file_id).await?;

    let path = storyboard_dir(cache_dir, media_file_id).join("index.vtt");
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| map_index_read_error(e, media_item_id))?;
    Ok(rewrite_vtt_sprite_urls(
        &content,
        media_item_id,
        media_file_id,
    ))
}

/// Read a WebP sprite sheet image for a media item's storyboard.
///
/// Validates the sprite filename against the expected pattern
/// (`sprite_NNN.webp`) via [`sb_svc::validate_sprite_filename`] to prevent
/// path traversal, then reads the file bytes. Returns
/// `InvalidSpriteFilename` for malformed names, `StoryboardNotFound`
/// when no storyboard exists yet.
pub async fn get_storyboard_sprite(
    pool: &PgPool,
    media_item_id: Uuid,
    requested_media_file_id: Option<Uuid>,
    sprite_filename: &str,
    cache_dir: &Path,
) -> Result<Vec<u8>, StoryboardError> {
    // Validate the filename before any DB or disk access — reject malformed
    // names cheaply and uniformly.
    let sheet_number = sb_svc::validate_sprite_filename(sprite_filename)
        .map_err(StoryboardError::InvalidSpriteFilename)?;

    let media_file_id =
        resolve_media_file_for_storyboard(pool, media_item_id, requested_media_file_id).await?;

    // Bounds-check the requested sheet against what was generated. The
    // `storyboards.sprite_count` column is authoritative.
    let sprite_count: i32 =
        sqlx::query_scalar("SELECT sprite_count FROM storyboards WHERE media_file_id = $1")
            .bind(media_file_id)
            .fetch_optional(pool)
            .await?
            .ok_or(StoryboardError::StoryboardNotFound { media_item_id })?;

    if sheet_number > sprite_count as u32 {
        return Err(StoryboardError::InvalidSpriteFilename(format!(
            "sprite {sheet_number} exceeds sprite_count {sprite_count}"
        )));
    }

    let path = storyboard_dir(cache_dir, media_file_id).join(sprite_filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|e| map_sprite_read_error(e, media_item_id, sprite_filename))?;
    Ok(bytes)
}

/// Trigger storyboard generation for all missing items in a library.
///
/// Verifies the library exists, then runs the worker synchronously and
/// returns a summary. Honors server-wide and per-library enablement: when
/// storyboards are disabled the result is an empty summary (not an error),
/// matching the segment domain's `trigger_library_analysis` pattern.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` SSE
/// events to the admin's channel so the UI can show live progress.
pub async fn trigger_library_generation(
    state: &AppState,
    library_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<GenerateStoryboardsResponse, StoryboardError> {
    verify_library_exists(&state.pool, library_id).await?;

    let result = crate::workers::storyboard_generator::generate_for_library_one(
        state,
        library_id,
        requesting_user_id,
    )
    .await
    .map_err(StoryboardError::Database)?;

    Ok(GenerateStoryboardsResponse {
        queued: false,
        message: result.message(),
    })
}

/// Trigger storyboard generation for a specific media item (force regen).
///
/// Resolves the item's primary media file and runs the worker on it. Unlike
/// the library endpoint this forces regeneration even if a storyboard
/// already exists (the worker deletes and regenerates). Returns
/// `MediaItemNotFound` when the item or its primary media file does not
/// exist.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` SSE
/// events to the admin's channel.
pub async fn trigger_item_generation(
    state: &AppState,
    media_item_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<GenerateStoryboardsResponse, StoryboardError> {
    let result = crate::workers::storyboard_generator::generate_for_item_one(
        state,
        media_item_id,
        requesting_user_id,
    )
    .await?;

    Ok(GenerateStoryboardsResponse {
        queued: false,
        message: result.message(),
    })
}

/// Delete cached storyboard data for a media item.
///
/// Removes the on-disk sprite files and the `storyboards` DB row. The
/// storyboard can be regenerated at any time — it is derived data stored in
/// the cache layer (per STORYBOARDS.md design principle 1). Returns
/// `Ok(())` even if the on-disk directory is already gone (idempotent); the
/// DB row deletion is the source of truth.
pub async fn delete_storyboard(
    pool: &PgPool,
    media_item_id: Uuid,
    cache_dir: &Path,
) -> Result<(), StoryboardError> {
    let media_file_id = resolve_media_file_for_storyboard(pool, media_item_id, None).await?;

    // DB row first — guarantees the HTTP 404 path is consistent even if the
    // disk directory is in a weird state. `RETURNING` keeps it atomic.
    let deleted: Option<Uuid> = sqlx::query_scalar(
        "DELETE FROM storyboards WHERE media_file_id = $1 RETURNING media_file_id",
    )
    .bind(media_file_id)
    .fetch_optional(pool)
    .await?;

    if deleted.is_none() {
        // No DB row — but a stray on-disk directory may still exist (e.g.
        // from a crashed generation). Clean it up so the delete is fully
        // idempotent, but return NotFound to mirror the row's absence.
        let dir = storyboard_dir(cache_dir, media_file_id);
        let _ = tokio::fs::remove_dir_all(&dir).await;
        return Err(StoryboardError::StoryboardNotFound { media_item_id });
    }

    // On-disk cleanup. Best-effort: a missing or read-only directory does
    // not invalidate the (already-committed) DB deletion. Logged at warn so
    // operators notice disk-state drift.
    let dir = storyboard_dir(cache_dir, media_file_id);
    if let Err(e) = tokio::fs::remove_dir_all(&dir).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            error = %e,
            dir = %dir.display(),
            "failed to remove storyboard cache directory; DB row was deleted"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve the primary media file for an item (largest healthy file).
///
/// Mirrors the playback domain's `resolve_media_file` selection — both
/// domains pick the same file, so the storyboard corresponds to the file
/// the user will actually stream.
async fn resolve_primary_media_file(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<Option<Uuid>, StoryboardError> {
    let row = sqlx::query(
        "SELECT mf.id \
         FROM media_files mf \
         JOIN media_items mi ON mi.id = mf.media_item_id \
         WHERE mf.media_item_id = $1 AND mf.is_healthy = true \
         ORDER BY mf.file_size DESC LIMIT 1",
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| r.get::<Uuid, _>("id")))
}

/// Verify a library exists and is not soft-deleted. Returns
/// `LibraryNotFound` (LIB_001, 404) otherwise. Shared by the trigger
/// functions to surface a clear error before the worker silently skips.
async fn verify_library_exists(pool: &PgPool, library_id: Uuid) -> Result<(), StoryboardError> {
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
            .bind(library_id)
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Err(StoryboardError::LibraryNotFound { library_id });
    }
    Ok(())
}

/// Resolve the requested healthy media file or the primary fallback,
/// requiring that its storyboard DB row exists.
async fn resolve_media_file_for_storyboard(
    pool: &PgPool,
    media_item_id: Uuid,
    requested_media_file_id: Option<Uuid>,
) -> Result<Uuid, StoryboardError> {
    let media_file_id = if let Some(media_file_id) = requested_media_file_id {
        let is_requested_file_healthy: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM media_files WHERE id = $1 AND media_item_id = $2 AND is_healthy = true)",
        )
        .bind(media_file_id)
        .bind(media_item_id)
        .fetch_one(pool)
        .await?;

        if !is_requested_file_healthy {
            return Err(StoryboardError::MediaFileNotFound { media_file_id });
        }

        media_file_id
    } else {
        resolve_primary_media_file(pool, media_item_id)
            .await?
            .ok_or(StoryboardError::MediaItemNotFound { media_item_id })?
    };

    // Confirm a storyboard row exists. We deliberately don't load the full
    // row here — callers (index/sprite/delete) only need the path, which is
    // derived from media_file_id. The sprite handler does its own
    // sprite_count bounds check.
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM storyboards WHERE media_file_id = $1)")
            .bind(media_file_id)
            .fetch_one(pool)
            .await?;

    if !exists {
        return Err(StoryboardError::StoryboardNotFound { media_item_id });
    }

    Ok(media_file_id)
}

/// The on-disk directory holding one media file's storyboard artifacts.
///
/// Layout: `{cache_dir}/storyboards/{media_file_id}/` per STORYBOARDS.md
/// "Storage Path" spec. `media_file_id` (not `media_item_id`) is used
/// because multi-version items (e.g. 4K + 1080p) may have different aspect
/// ratios, requiring separate storyboards per file.
fn storyboard_dir(cache_dir: &Path, media_file_id: Uuid) -> PathBuf {
    cache_dir
        .join(STORYBOARDS_SUBDIR)
        .join(media_file_id.to_string())
}

/// Build the relative URL for a sprite sheet, served by the sprite HTTP
/// handler at `/api/v1/items/{item_id}/storyboard/{sprite}`.
fn sprite_url_for(media_item_id: Uuid, media_file_id: Uuid, sheet_index: u32) -> String {
    let sprite_filename = sb_svc::sprite_filename(sheet_index);
    sprite_url_for_filename(media_item_id, media_file_id, &sprite_filename)
}

fn sprite_url_for_filename(
    media_item_id: Uuid,
    media_file_id: Uuid,
    sprite_filename: &str,
) -> String {
    format!(
        "/api/v1/items/{media_item_id}/storyboard/{sprite_filename}?media_file_id={media_file_id}"
    )
}

/// Build the relative URL for the WebVTT index file.
fn index_url_for(media_item_id: Uuid, media_file_id: Uuid) -> String {
    format!("/api/v1/items/{media_item_id}/storyboard/index.vtt?media_file_id={media_file_id}")
}

fn rewrite_vtt_sprite_urls(content: &str, media_item_id: Uuid, media_file_id: Uuid) -> String {
    content
        .lines()
        .map(|line| {
            let Some((sprite_filename, fragment)) = line.split_once("#xywh=") else {
                return line.to_owned();
            };
            if sb_svc::validate_sprite_filename(sprite_filename).is_err() {
                return line.to_owned();
            }
            format!(
                "{}#xywh={fragment}",
                sprite_url_for_filename(media_item_id, media_file_id, sprite_filename)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if content.ends_with('\n') { "\n" } else { "" }
}

/// Read the per-generation grid shape from the storyboard row's metadata
/// JSONB. The worker writes `metadata.columns` and `metadata.rows` when it
/// creates the row; this falls back to the design defaults (10×20) when
/// absent so older or externally-authored rows still serve cleanly.
fn read_grid_shape(metadata: &serde_json::Value) -> (i32, i32) {
    let default = (10, 20);
    let Some(obj) = metadata.as_object() else {
        return default;
    };
    let columns = obj
        .get("columns")
        .and_then(|v| v.as_i64())
        .filter(|&n| n > 0 && n < i32::MAX as i64)
        .map(|n| n as i32)
        .unwrap_or(default.0);
    let rows = obj
        .get("rows")
        .and_then(|v| v.as_i64())
        .filter(|&n| n > 0 && n < i32::MAX as i64)
        .map(|n| n as i32)
        .unwrap_or(default.1);
    (columns, rows)
}

/// Compute the number of thumbnails in sheet `sheet_index` (0-based) given
/// the per-sheet capacity and the total thumbnail count. The final sheet
/// may be partial.
fn thumbnails_in_sheet(
    sheet_index: i32,
    _sprite_count: i32,
    thumbnails_per_sheet: i32,
    total_thumbnails: i32,
) -> i32 {
    let start = sheet_index * thumbnails_per_sheet;
    ((total_thumbnails - start).max(0)).min(thumbnails_per_sheet)
}

/// Translate a `read_to_string` error on the index file into the domain
/// error. `NotFound` → `StoryboardNotFound` (the storyboard exists in the
/// DB but its files are missing — rare but recoverable by regeneration);
/// other IO errors surface as `Database` (Internal) since they indicate
/// filesystem permissions or hardware problems.
fn map_index_read_error(e: std::io::Error, media_item_id: Uuid) -> StoryboardError {
    if e.kind() == std::io::ErrorKind::NotFound {
        StoryboardError::StoryboardNotFound { media_item_id }
    } else {
        tracing::error!(
            error = %e,
            %media_item_id,
            "failed to read storyboard index.vtt"
        );
        // sqlx::Error::Io would be the conventional mapping, but this is a
        // tokio::fs error outside sqlx. Wrap as a generic DB-class error so
        // the HTTP layer surfaces 500 (Internal) per ERROR_HANDLING.md.
        StoryboardError::Database(sqlx::Error::PoolClosed)
    }
}

/// Translate a sprite file read error. Same pattern as
/// [`map_index_read_error`] but includes the sprite filename in the log.
fn map_sprite_read_error(
    e: std::io::Error,
    media_item_id: Uuid,
    sprite_filename: &str,
) -> StoryboardError {
    if e.kind() == std::io::ErrorKind::NotFound {
        StoryboardError::StoryboardNotFound { media_item_id }
    } else {
        tracing::error!(
            error = %e,
            %media_item_id,
            %sprite_filename,
            "failed to read storyboard sprite file"
        );
        StoryboardError::Database(sqlx::Error::PoolClosed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storyboard_dir_layout() {
        let cache = Path::new("/var/cache/duskcue");
        let id = Uuid::nil();
        let dir = storyboard_dir(cache, id);
        assert_eq!(
            dir,
            Path::new("/var/cache/duskcue/storyboards/00000000-0000-0000-0000-000000000000")
        );
    }

    #[test]
    fn sprite_url_format() {
        let url = sprite_url_for(Uuid::nil(), Uuid::from_u128(1), 0);
        assert_eq!(
            url,
            "/api/v1/items/00000000-0000-0000-0000-000000000000/storyboard/sprite_001.webp?media_file_id=00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn index_url_format() {
        let url = index_url_for(Uuid::nil(), Uuid::from_u128(1));
        assert_eq!(
            url,
            "/api/v1/items/00000000-0000-0000-0000-000000000000/storyboard/index.vtt?media_file_id=00000000-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn vtt_sprite_urls_include_the_selected_media_file() {
        let media_item_id = Uuid::nil();
        let media_file_id = Uuid::from_u128(1);
        let content = "WEBVTT\n\n00:00:00.000 --> 00:00:10.000\nsprite_001.webp#xywh=0,0,320,180\n";
        let rewritten = rewrite_vtt_sprite_urls(&content, media_item_id, media_file_id);

        assert!(rewritten.contains(
            "/api/v1/items/00000000-0000-0000-0000-000000000000/storyboard/sprite_001.webp?media_file_id=00000000-0000-0000-0000-000000000001#xywh=0,0,320,180"
        ));
        assert!(rewritten.ends_with('\n'));
    }

    #[test]
    fn read_grid_shape_defaults_when_empty() {
        let v = serde_json::json!({});
        assert_eq!(read_grid_shape(&v), (10, 20));
    }

    #[test]
    fn read_grid_shape_reads_columns_rows() {
        let v = serde_json::json!({"columns": 5, "rows": 10});
        assert_eq!(read_grid_shape(&v), (5, 10));
    }

    #[test]
    fn read_grid_shape_rejects_zero_and_negative() {
        let v = serde_json::json!({"columns": 0, "rows": -1});
        assert_eq!(read_grid_shape(&v), (10, 20));
    }

    #[test]
    fn read_grid_shape_handles_non_object() {
        let v = serde_json::json!(42);
        assert_eq!(read_grid_shape(&v), (10, 20));
        let v = serde_json::json!(null);
        assert_eq!(read_grid_shape(&v), (10, 20));
    }

    #[test]
    fn thumbnails_in_sheet_full_sheets() {
        // 720 thumbnails, 200 per sheet.
        for i in 0..3 {
            assert_eq!(thumbnails_in_sheet(i, 4, 200, 720), 200);
        }
    }

    #[test]
    fn thumbnails_in_sheet_partial_last() {
        // 720 thumbnails, 200 per sheet → sheet 3 has 120.
        assert_eq!(thumbnails_in_sheet(3, 4, 200, 720), 120);
    }

    #[test]
    fn thumbnails_in_sheet_exact_multiple() {
        // 200 thumbnails, 200 per sheet → only one sheet, no partial.
        assert_eq!(thumbnails_in_sheet(0, 1, 200, 200), 200);
    }

    #[test]
    fn thumbnails_in_sheet_oob_returns_zero() {
        assert_eq!(thumbnails_in_sheet(99, 4, 200, 720), 0);
    }

    #[test]
    fn map_index_read_error_not_found_maps_to_storyboard_not_found() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let id = Uuid::nil();
        assert!(matches!(
            map_index_read_error(err, id),
            StoryboardError::StoryboardNotFound { .. }
        ));
    }

    #[test]
    fn map_sprite_read_error_not_found_maps_to_storyboard_not_found() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let id = Uuid::nil();
        assert!(matches!(
            map_sprite_read_error(err, id, "sprite_001.webp"),
            StoryboardError::StoryboardNotFound { .. }
        ));
    }
}
