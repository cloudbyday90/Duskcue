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

//! Segment domain service — DB CRUD for the `media_segments` table plus the
//! `trigger_library_analysis` stub reserved for Task 5 (the worker).
//!
//! The detection pipeline itself lives in
//! [`crate::services::segments`](../../services/segments/index.html). The
//! worker (Task 5) will be the integration point that calls the detection
//! pipeline and writes results via the CRUD functions here.
//!
//! All queries use runtime `sqlx::query` (not compile-time `query!`)
//! consistent with the auth/users/etc. domain convention — no `DATABASE_URL`
//! is required at build time.

use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;

use crate::domains::segments::error::SegmentError;
use crate::domains::segments::types::*;
use crate::state::AppState;
use crate::workers::segment_detector;

/// List all segments for a media item, optionally filtered by type.
///
/// Verifies the media item exists (returns `MediaItemNotFound` otherwise).
/// The `can_edit` flag is computed by the caller from the requesting user's
/// capabilities and propagated into every row — the service layer is
/// user-agnostic per the design's `can_edit` decision.
pub async fn list_segments(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_type_filter: Option<&str>,
    can_edit: bool,
) -> Result<SegmentListResponse, SegmentError> {
    if let Some(stype) = segment_type_filter
        && !VALID_SEGMENT_TYPES.contains(&stype)
    {
        return Err(SegmentError::InvalidSegmentType(stype.to_string()));
    }

    ensure_media_item_exists(pool, media_item_id).await?;

    let rows = if let Some(stype) = segment_type_filter {
        sqlx::query(
            r#"
            SELECT id, media_item_id, segment_type, start_ms, end_ms,
                   skip_to_ms, confidence, source, is_manual
            FROM media_segments
            WHERE media_item_id = $1 AND segment_type = $2
            ORDER BY start_ms ASC
            "#,
        )
        .bind(media_item_id)
        .bind(stype)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            SELECT id, media_item_id, segment_type, start_ms, end_ms,
                   skip_to_ms, confidence, source, is_manual
            FROM media_segments
            WHERE media_item_id = $1
            ORDER BY start_ms ASC
            "#,
        )
        .bind(media_item_id)
        .fetch_all(pool)
        .await?
    };

    let segments = rows.iter().map(|r| row_to_response(r, can_edit)).collect();

    Ok(SegmentListResponse { segments })
}

/// Create a manual segment.
///
/// Validates the segment type against the DB CHECK constraint list,
/// validates timestamps (`end_ms > start_ms`, `skip_to_ms ∈ [start_ms,
/// end_ms]`), and inserts with `source='manual'`, `is_manual=true`,
/// `confidence = req.confidence.unwrap_or(1.0)` (manual segments are
/// authoritative), and `skip_to_ms` defaulting to `end_ms`. Maps the
/// `(media_item_id, segment_type)` partial-unique-index violation to
/// `ManualSegmentExists`.
pub async fn create_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    req: &CreateSegmentRequest,
) -> Result<SegmentResponse, SegmentError> {
    let segment_type = req
        .segment_type
        .as_ref()
        .ok_or_else(|| SegmentError::InvalidSegmentType("<missing>".into()))?;
    if !VALID_SEGMENT_TYPES.contains(&segment_type.as_str()) {
        return Err(SegmentError::InvalidSegmentType(segment_type.clone()));
    }

    let start_ms = req.start_ms;
    let end_ms = req.end_ms;
    let skip_to_ms = req.skip_to_ms.unwrap_or(end_ms);
    let confidence = req.confidence.unwrap_or(1.0).clamp(0.0, 1.0);

    validate_timestamps(start_ms, end_ms, skip_to_ms)?;

    ensure_media_item_exists(pool, media_item_id).await?;

    let row = sqlx::query(
        r#"
        INSERT INTO media_segments
            (media_item_id, segment_type, start_ms, end_ms, skip_to_ms,
             confidence, source, is_manual, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, 'manual', true, '{}'::jsonb)
        RETURNING id, media_item_id, segment_type, start_ms, end_ms,
                  skip_to_ms, confidence, source, is_manual
        "#,
    )
    .bind(media_item_id)
    .bind(segment_type)
    .bind(start_ms)
    .bind(end_ms)
    .bind(skip_to_ms)
    .bind(confidence)
    .fetch_one(pool)
    .await
    .map_err(|e| map_unique_violation(e, segment_type))?;

    Ok(row_to_response(&row, true))
}

/// Update a manual or auto-detected segment via partial replacement.
///
/// Each field uses the prior value when omitted from the request. After
/// applying the partial update the function re-validates the resulting
/// `(start_ms, end_ms, skip_to_ms)` triple against the DB CHECK constraints
/// to catch incoherent partial updates.
pub async fn update_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_id: Uuid,
    req: &UpdateSegmentRequest,
) -> Result<SegmentResponse, SegmentError> {
    let current = sqlx::query(
        r#"
        SELECT start_ms, end_ms, skip_to_ms
        FROM media_segments
        WHERE id = $1 AND media_item_id = $2
        "#,
    )
    .bind(segment_id)
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    let (cur_start, cur_end, cur_skip): (i32, i32, i32) = match current {
        Some(ref r) => (r.get("start_ms"), r.get("end_ms"), r.get("skip_to_ms")),
        None => {
            ensure_media_item_exists(pool, media_item_id).await?;
            return Err(SegmentError::SegmentNotFound { segment_id });
        }
    };

    let new_start = req.start_ms.unwrap_or(cur_start);
    let new_end = req.end_ms.unwrap_or(cur_end);
    let new_skip = req.skip_to_ms.unwrap_or(cur_skip);
    let new_confidence = req.confidence.map(|c| c.clamp(0.0, 1.0));

    validate_timestamps(new_start, new_end, new_skip)?;

    let row = sqlx::query(
        r#"
        UPDATE media_segments
        SET
            start_ms     = $3,
            end_ms       = $4,
            skip_to_ms   = $5,
            confidence   = COALESCE($6, confidence),
            updated_at   = now()
        WHERE id = $1 AND media_item_id = $2
        RETURNING id, media_item_id, segment_type, start_ms, end_ms,
                  skip_to_ms, confidence, source, is_manual
        "#,
    )
    .bind(segment_id)
    .bind(media_item_id)
    .bind(new_start)
    .bind(new_end)
    .bind(new_skip)
    .bind(new_confidence)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => SegmentError::SegmentNotFound { segment_id },
        other => SegmentError::Database(other),
    })?;

    Ok(row_to_response(&row, true))
}

/// Delete a segment by id (scoped to `media_item_id`).
///
/// Returns `SegmentNotFound` if the segment does not exist for the given
/// item, or `MediaItemNotFound` if the media item itself does not exist.
pub async fn delete_segment(
    pool: &PgPool,
    media_item_id: Uuid,
    segment_id: Uuid,
) -> Result<(), SegmentError> {
    ensure_media_item_exists(pool, media_item_id).await?;

    let result = sqlx::query("DELETE FROM media_segments WHERE id = $1 AND media_item_id = $2")
        .bind(segment_id)
        .bind(media_item_id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(SegmentError::SegmentNotFound { segment_id });
    }
    Ok(())
}

/// Trigger segment analysis for a library.
///
/// Runs the detection pipeline synchronously (matching the `scan_library`
/// pattern from Phase 5 Task 5) and returns a summary of what was detected.
/// The scheduled `segment_analysis` task iterates all libraries via the
/// scheduler; this function services the per-library admin trigger endpoint.
///
/// Returns `LibraryNotFound` if the library does not exist or is soft-deleted.
pub async fn trigger_library_analysis(
    state: &AppState,
    library_id: Uuid,
) -> Result<AnalyzeSegmentsResponse, SegmentError> {
    verify_library_exists(&state.pool, library_id).await?;

    let result = segment_detector::analyze_library_one(state, library_id)
        .await
        .map_err(SegmentError::Database)?;

    Ok(AnalyzeSegmentsResponse {
        library_id,
        queued: false,
        message: result.message(),
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn ensure_media_item_exists(pool: &PgPool, media_item_id: Uuid) -> Result<(), SegmentError> {
    let exists: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM media_items WHERE id = $1")
        .bind(media_item_id)
        .fetch_optional(pool)
        .await?;
    if exists.is_none() {
        return Err(SegmentError::MediaItemNotFound { media_item_id });
    }
    Ok(())
}

async fn verify_library_exists(pool: &PgPool, library_id: Uuid) -> Result<(), SegmentError> {
    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM libraries WHERE id = $1 AND deleted_at IS NULL")
            .bind(library_id)
            .fetch_optional(pool)
            .await?;
    if exists.is_none() {
        return Err(SegmentError::LibraryNotFound { library_id });
    }
    Ok(())
}

fn validate_timestamps(start_ms: i32, end_ms: i32, skip_to_ms: i32) -> Result<(), SegmentError> {
    if end_ms <= start_ms {
        return Err(SegmentError::InvalidTimestamps {
            start_ms,
            end_ms,
            skip_to_ms,
        });
    }
    if skip_to_ms < start_ms || skip_to_ms > end_ms {
        return Err(SegmentError::InvalidTimestamps {
            start_ms,
            end_ms,
            skip_to_ms,
        });
    }
    Ok(())
}

fn row_to_response(row: &sqlx::postgres::PgRow, can_edit: bool) -> SegmentResponse {
    SegmentResponse {
        id: row.get("id"),
        media_item_id: row.get("media_item_id"),
        segment_type: row.get("segment_type"),
        start_ms: row.get("start_ms"),
        end_ms: row.get("end_ms"),
        skip_to_ms: row.get("skip_to_ms"),
        confidence: row.get("confidence"),
        source: row.get("source"),
        is_manual: row.get("is_manual"),
        can_edit,
    }
}

fn map_unique_violation(e: sqlx::Error, segment_type: &str) -> SegmentError {
    if let Some(db) = e.as_database_error()
        && db.is_unique_violation()
    {
        return SegmentError::ManualSegmentExists {
            segment_type: segment_type.to_string(),
        };
    }
    SegmentError::Database(e)
}
