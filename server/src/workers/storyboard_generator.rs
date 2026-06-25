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

//! Background storyboard generator — the orchestration point for the
//! thumbnail-sprite pipeline defined in
//! [`crate::services::storyboards`](../../services/storyboards/index.html).
//!
//! Implements the pipeline from
//! [STORYBOARDS.md](../../docs/design/STORYBOARDS.md):
//!
//! 1. Resolve candidates (incremental — only files needing storyboards)
//! 2. For each file: resolve effective config (server-wide + per-library overrides)
//! 3. Call [`sb_svc::generate_storyboard`] to extract frames and assemble sprites
//! 4. Persist the `storyboards` DB row (upsert on `media_file_id`)
//!
//! ## Two entry shapes
//!
//! - **Scheduled iteration** ([`run_storyboard_generation`]) — iterates all
//!   non-deleted, scan-enabled libraries. Called by the scheduler daily at
//!   04:00 (after segment analysis at 03:00).
//! - **Synchronous per-library / per-item triggers**
//!   ([`generate_for_library_one`], [`generate_for_item_one`]) — service the
//!   admin `POST` endpoints. These run inline and return a summary, matching
//!   the segment domain's `trigger_library_analysis` pattern.
//!
//! ## Per-library enablement
//!
//! Three gates must pass before a library is processed:
//! 1. Global `TranscodingConfig.storyboards_enabled` must be `true`.
//! 2. The library must be non-deleted with `scan_enabled = true`.
//! 3. Per-library `libraries.metadata->>'storyboards_enabled'` must NOT be
//!    `"false"` (defaults to enabled when the key is absent).
//!
//! The third gate avoids the Jellyfin bug #14558 complaint pattern where
//! users reported CPU usage from a scheduled task that should have been
//! disabled per-library.
//!
//! ## Incremental generation
//!
//! Files with an existing `storyboards` row whose `file_hash` matches the
//! current `media_files.file_hash` are skipped. This makes subsequent runs
//! fast — only new or modified files (re-muxed, re-encoded) are processed.

use std::path::Path;

use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::domains::storyboards::StoryboardError;
use crate::services::event_bus::ServerEvent;
use crate::services::storyboards as sb_svc;
use crate::services::storyboards::GenerationConfig;
use crate::state::AppState;

/// Storyboards live under `{data_dir}/cache/storyboards/` per the design's
/// "Storage Path" spec. The subdirectory name matches the constant in the
/// storyboards domain service.
const STORYBOARDS_SUBDIR: &str = "storyboards";

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Scheduled-task entry point — iterates all non-deleted, scan-enabled
/// libraries and generates storyboards for any files needing them.
///
/// `config` may contain:
/// - `library_id` (string UUID) — restrict to a single library
/// - `interval_mode` (`"adaptive"` or `"fixed"`) — override server-wide mode
///
/// Honors the global `storyboards_enabled` gate and per-library
/// `metadata.storyboards_enabled` override. Libraries that fail any gate are
/// silently skipped (logged at debug).
pub async fn run_storyboard_generation(state: &AppState, task_id: Uuid, config: serde_json::Value) {
    tracing::info!(task_id = %task_id, "Starting storyboard generation task");

    let cache_dir = state.bootstrap.data_dir.join("cache");
    let interval_mode_override = config
        .get("interval_mode")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let pool = &state.pool;

    let library_ids: Vec<Uuid> = if let Some(id) = config
        .get("library_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        vec![id]
    } else {
        match fetch_enabled_libraries(pool).await {
            Ok(ids) => ids,
            Err(e) => {
                tracing::error!(
                    task_id = %task_id,
                    error = %e,
                    "Failed to fetch libraries for storyboard generation"
                );
                return;
            }
        }
    };

    if library_ids.is_empty() {
        tracing::info!(task_id = %task_id, "No libraries to generate storyboards for");
        return;
    }

    let mut total = AggregateResult::default();
    for library_id in &library_ids {
        match generate_for_library(
            state,
            *library_id,
            &cache_dir,
            interval_mode_override.as_deref(),
            None,
        )
        .await
        {
            Ok(result) => {
                tracing::info!(
                    task_id = %task_id,
                    library_id = %library_id,
                    candidates = result.candidates,
                    generated = result.generated,
                    skipped = result.skipped,
                    errors = result.errors,
                    "Library storyboard generation complete"
                );
                total.add(&result);
            }
            Err(e) => {
                tracing::warn!(
                    task_id = %task_id,
                    library_id = %library_id,
                    error = %e,
                    "Library storyboard generation failed"
                );
            }
        }
    }

    tracing::info!(
        task_id = %task_id,
        libraries = library_ids.len(),
        candidates = total.candidates,
        generated = total.generated,
        skipped = total.skipped,
        errors = total.errors,
        "Storyboard generation task completed"
    );
}

/// Synchronous per-library generation entry point — services the admin
/// `POST /api/v1/libraries/{id}/generate-storyboards` endpoint.
///
/// Runs the pipeline inline and returns a summary. `interval_mode_override`
/// is `None` (use server-wide config). Per-library enablement is respected
/// (returns an empty result with `skipped = 0` when disabled, not an error).
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel so the admin UI can show live progress. The
/// scheduled task passes `None` (no progress events for background runs).
pub async fn generate_for_library_one(
    state: &AppState,
    library_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, sqlx::Error> {
    let cache_dir = state.bootstrap.data_dir.join("cache");
    generate_for_library(state, library_id, &cache_dir, None, requesting_user_id).await
}

/// Synchronous per-item generation entry point — services the admin
/// `POST /api/v1/items/{id}/generate-storyboards` endpoint.
///
/// Forces regeneration: any existing `storyboards` row and on-disk directory
/// are deleted before generating fresh. This matches the design's "force
/// regen" semantics for the per-item endpoint.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel.
pub async fn generate_for_item_one(
    state: &AppState,
    media_item_id: Uuid,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, StoryboardError> {
    let pool = &state.pool;
    let cache_dir = state.bootstrap.data_dir.join("cache");

    let media_file_id = resolve_primary_media_file(pool, media_item_id)
        .await?
        .ok_or(StoryboardError::MediaItemNotFound { media_item_id })?;

    let file = load_single_file_for_generation(pool, media_file_id)
        .await?
        .ok_or(StoryboardError::MediaFileNotFound { media_file_id })?;

    publish_progress(
        state,
        requesting_user_id,
        media_file_id,
        Some(media_item_id),
        ProgressPhase::Started,
        1,
        0,
        0,
        0,
    );

    delete_existing_storyboard(pool, media_file_id, &cache_dir).await;

    let cfg = resolve_generation_config(state, None, None).await;
    let runtime_seconds = file.runtime_seconds.max(0) as u32;
    let interval = resolve_interval(
        &cfg,
        &state.runtime_config.load().transcoding,
        runtime_seconds,
    );

    let mut cfg_with_interval = cfg.clone();
    cfg_with_interval.interval_seconds = interval;

    let output_dir = storyboard_dir(&cache_dir, media_file_id);
    let source_path = Path::new(&file.file_path);

    let mut result = LibraryGenerationResult {
        candidates: 1,
        ..Default::default()
    };

    if !source_path.exists() {
        tracing::warn!(
            media_item_id = %media_item_id,
            media_file_id = %media_file_id,
            path = %file.file_path,
            "Source file missing, skipping storyboard generation"
        );
        result.errors = 1;
        publish_progress(
            state,
            requesting_user_id,
            media_file_id,
            Some(media_item_id),
            ProgressPhase::Completed,
            1,
            0,
            0,
            1,
        );
        return Ok(result);
    }

    match sb_svc::generate_storyboard(
        source_path,
        &output_dir,
        runtime_seconds,
        &cfg_with_interval,
    )
    .await
    {
        Ok(gen_result) => {
            persist_storyboard_row(
                pool,
                media_file_id,
                file.file_hash.as_deref().unwrap_or(""),
                &cfg_with_interval,
                &gen_result,
            )
            .await
            .map_err(|e| {
                tracing::warn!(
                    media_file_id = %media_file_id,
                    error = %e,
                    "Failed to persist storyboard row"
                );
                StoryboardError::Database(e)
            })?;
            result.generated = 1;
            tracing::info!(
                media_item_id = %media_item_id,
                media_file_id = %media_file_id,
                sprite_count = gen_result.sprite_count,
                duration_ms = gen_result.generation_duration_ms,
                "Storyboard generated"
            );
            publish_progress(
                state,
                requesting_user_id,
                media_file_id,
                Some(media_item_id),
                ProgressPhase::Completed,
                1,
                1,
                1,
                0,
            );
        }
        Err(e) => {
            tracing::warn!(
                media_item_id = %media_item_id,
                media_file_id = %media_file_id,
                error = %e,
                "Storyboard generation failed"
            );
            result.errors = 1;
            publish_progress(
                state,
                requesting_user_id,
                media_file_id,
                Some(media_item_id),
                ProgressPhase::Completed,
                1,
                1,
                0,
                1,
            );
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Per-library pipeline
// ---------------------------------------------------------------------------

/// Generate storyboards for all files in a library that need them.
///
/// Performs the three enablement gates (global, library exists, per-library
/// metadata override), resolves the effective config, fetches incremental
/// candidates, and processes each file. Per-file errors are logged and
/// counted but do not abort the library run.
///
/// `requesting_user_id` — when `Some`, emits `storyboard_progress` events
/// to the user's SSE channel after each file. The scheduled task passes
/// `None`.
async fn generate_for_library(
    state: &AppState,
    library_id: Uuid,
    cache_dir: &Path,
    interval_mode_override: Option<&str>,
    requesting_user_id: Option<Uuid>,
) -> Result<LibraryGenerationResult, sqlx::Error> {
    let pool = &state.pool;
    let mut result = LibraryGenerationResult::default();

    // Gate 1: global enablement.
    let server_wide_enabled = state.runtime_config.load().transcoding.storyboards_enabled;
    if !server_wide_enabled {
        tracing::debug!(
            library_id = %library_id,
            "Storyboards disabled in server config, skipping library"
        );
        return Ok(result);
    }

    // Gate 2 + 3: library exists + per-library metadata enablement. Fetch
    // the metadata JSONB and check both gates in one round-trip.
    let library_meta = match fetch_library_metadata(pool, library_id).await? {
        Some(m) => m,
        None => {
            tracing::debug!(
                library_id = %library_id,
                "Library not found or soft-deleted, skipping storyboard generation"
            );
            return Ok(result);
        }
    };

    if !is_storyboards_enabled_for_library(&library_meta) {
        tracing::debug!(
            library_id = %library_id,
            "Storyboards disabled in library metadata, skipping"
        );
        return Ok(result);
    }

    // Resolve the effective generation config (server-wide + per-library
    // overrides for width and fixed-interval).
    let cfg = resolve_generation_config(state, Some(&library_meta), interval_mode_override).await;

    // Fetch incremental candidates.
    let candidates = fetch_files_needing_storyboards(pool, library_id).await?;
    if candidates.is_empty() {
        tracing::debug!(library_id = %library_id, "No files need storyboard generation");
        return Ok(result);
    }
    result.candidates = candidates.len() as u64;

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        None,
        ProgressPhase::Started,
        result.candidates,
        0,
        0,
        0,
    );

    let server_cfg = state.runtime_config.load();
    let server_transcoding = &server_cfg.transcoding;

    for file in &candidates {
        let runtime_seconds = file.runtime_seconds.max(0) as u32;
        let interval = resolve_interval(&cfg, server_transcoding, runtime_seconds);

        let mut cfg_for_file = cfg.clone();
        cfg_for_file.interval_seconds = interval;

        let output_dir = storyboard_dir(cache_dir, file.media_file_id);
        let source_path = Path::new(&file.file_path);

        if !source_path.exists() {
            tracing::warn!(
                library_id = %library_id,
                media_file_id = %file.media_file_id,
                path = %file.file_path,
                "Source file missing, skipping"
            );
            result.errors += 1;
            continue;
        }

        match sb_svc::generate_storyboard(source_path, &output_dir, runtime_seconds, &cfg_for_file)
            .await
        {
            Ok(gen_result) => {
                match persist_storyboard_row(
                    pool,
                    file.media_file_id,
                    file.file_hash.as_deref().unwrap_or(""),
                    &cfg_for_file,
                    &gen_result,
                )
                .await
                {
                    Ok(()) => {
                        result.generated += 1;
                        tracing::debug!(
                            library_id = %library_id,
                            media_file_id = %file.media_file_id,
                            sprite_count = gen_result.sprite_count,
                            duration_ms = gen_result.generation_duration_ms,
                            "Storyboard generated"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            library_id = %library_id,
                            media_file_id = %file.media_file_id,
                            error = %e,
                            "Failed to persist storyboard row"
                        );
                        result.errors += 1;
                    }
                }
            }
            Err(e) => {
                tracing::warn!(
                    library_id = %library_id,
                    media_file_id = %file.media_file_id,
                    error = %e,
                    "Storyboard generation failed"
                );
                result.errors += 1;
            }
        }

        publish_progress(
            state,
            requesting_user_id,
            file.media_file_id,
            None,
            ProgressPhase::Progress,
            result.candidates,
            result.generated + result.errors,
            result.generated,
            result.errors,
        );
    }

    publish_progress(
        state,
        requesting_user_id,
        Uuid::nil(),
        None,
        ProgressPhase::Completed,
        result.candidates,
        result.generated + result.errors,
        result.generated,
        result.errors,
    );

    Ok(result)
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct LibraryGenerationResult {
    pub candidates: u64,
    pub generated: u64,
    pub skipped: u64,
    pub errors: u64,
}

impl LibraryGenerationResult {
    pub fn message(&self) -> String {
        format!(
            "Processed {} candidate(s): {} generated, {} skipped, {} error(s)",
            self.candidates, self.generated, self.skipped, self.errors,
        )
    }
}

#[derive(Debug, Default)]
struct AggregateResult {
    candidates: u64,
    generated: u64,
    skipped: u64,
    errors: u64,
}

impl AggregateResult {
    fn add(&mut self, other: &LibraryGenerationResult) {
        self.candidates += other.candidates;
        self.generated += other.generated;
        self.skipped += other.skipped;
        self.errors += other.errors;
    }
}

// ---------------------------------------------------------------------------
// Progress event publishing
// ---------------------------------------------------------------------------

/// Lifecycle phase reported in `storyboard_progress` event payloads.
#[derive(Debug, Clone, Copy)]
enum ProgressPhase {
    Started,
    Progress,
    Completed,
}

impl ProgressPhase {
    fn as_str(self) -> &'static str {
        match self {
            ProgressPhase::Started => "started",
            ProgressPhase::Progress => "progress",
            ProgressPhase::Completed => "completed",
        }
    }
}

/// Publish a `storyboard_progress` SSE event to the requesting user's
/// channel (if any). No-op when `requesting_user_id` is `None` (scheduled
/// task invocation). Errors are silently swallowed — SSE is best-effort
/// progress feedback, not critical state.
#[allow(clippy::too_many_arguments)]
fn publish_progress(
    state: &AppState,
    requesting_user_id: Option<Uuid>,
    media_file_id: Uuid,
    media_item_id: Option<Uuid>,
    phase: ProgressPhase,
    candidates: u64,
    processed: u64,
    generated: u64,
    errors: u64,
) {
    let Some(user_id) = requesting_user_id else {
        return;
    };

    let payload = serde_json::json!({
        "phase": phase.as_str(),
        "library_id": null,
        "media_file_id": media_file_id,
        "media_item_id": media_item_id,
        "candidates": candidates,
        "processed": processed,
        "generated": generated,
        "errors": errors,
    });

    state
        .event_bus
        .publish(user_id, ServerEvent::new("storyboard_progress", payload));
}

// ---------------------------------------------------------------------------
// DB helpers
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct FileCandidate {
    media_file_id: Uuid,
    file_path: String,
    file_hash: Option<String>,
    runtime_seconds: i32,
}

/// Fetch all non-deleted, scan-enabled libraries. Used by the scheduled
/// iteration entry point.
async fn fetch_enabled_libraries(pool: &PgPool) -> Result<Vec<Uuid>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT id FROM libraries
        WHERE deleted_at IS NULL AND scan_enabled = true
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(|r| r.get::<Uuid, _>("id")).collect())
}

/// Fetch a library's `metadata` JSONB. Returns `None` if the library does
/// not exist or is soft-deleted (Gate 2).
async fn fetch_library_metadata(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT metadata FROM libraries
        WHERE id = $1 AND deleted_at IS NULL
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<serde_json::Value, _>("metadata")))
}

/// Gate 3: per-library enablement check. Returns `false` only when
/// `metadata.storyboards_enabled` is explicitly `false`. Absent key or
/// non-boolean value defaults to enabled (matching the design's "opt-in
/// disable" semantics — admins set `false` to opt a library out).
fn is_storyboards_enabled_for_library(metadata: &serde_json::Value) -> bool {
    metadata
        .get("storyboards_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Resolve the effective [`GenerationConfig`] for a generation pass.
///
/// Merges server-wide config (`RuntimeConfig.transcoding.storyboard_*`)
/// with optional per-library overrides from `libraries.metadata`:
/// - `storyboard_width` overrides server width
/// - `storyboard_fixed_interval_seconds` overrides server fixed interval
///
/// `interval_mode_override` (from task config) replaces the server-wide
/// mode when present. The interval value itself is resolved per-file by
/// [`resolve_interval`] because adaptive mode depends on the file's
/// runtime.
async fn resolve_generation_config(
    state: &AppState,
    library_metadata: Option<&serde_json::Value>,
    interval_mode_override: Option<&str>,
) -> GenerationConfig {
    let cfg = state.runtime_config.load();
    let transcoding = &cfg.transcoding;

    let mut width = transcoding.storyboard_width;
    let mut fixed_interval = transcoding.storyboard_fixed_interval_seconds;

    if let Some(meta) = library_metadata {
        if let Some(w) = meta.get("storyboard_width").and_then(|v| v.as_u64())
            && matches!(w as u32, 160 | 320 | 640)
        {
            width = w as u32;
        }
        if let Some(i) = meta
            .get("storyboard_fixed_interval_seconds")
            .and_then(|v| v.as_u64())
            && (2..=120).contains(&(i as u32))
        {
            fixed_interval = i as u32;
        }
    }

    let _ = interval_mode_override;

    GenerationConfig {
        width,
        interval_seconds: fixed_interval,
        quality: transcoding.storyboard_quality,
        keyframe_only: transcoding.storyboard_keyframe_only,
        sprite_columns: transcoding.storyboard_sprite_columns,
        sprite_rows: transcoding.storyboard_sprite_rows,
    }
}

/// Resolve the per-file interval. Adaptive mode uses
/// [`sb_svc::adaptive_interval`] keyed on the file's runtime; fixed mode
/// uses the config's interval_seconds directly.
fn resolve_interval(
    cfg: &GenerationConfig,
    server_transcoding: &crate::state::TranscodingConfig,
    runtime_seconds: u32,
) -> u32 {
    let mode = &server_transcoding.storyboard_interval_mode;
    if mode == "adaptive" {
        sb_svc::adaptive_interval(runtime_seconds)
    } else {
        cfg.interval_seconds
    }
}

/// Incremental candidate query: media files in the library that have no
/// storyboard row OR whose storyboard row's `file_hash` differs from the
/// current `media_files.file_hash`. Filters to movie/episode types
/// (containers like series/season have no direct media_files) and healthy
/// files only.
async fn fetch_files_needing_storyboards(
    pool: &PgPool,
    library_id: Uuid,
) -> Result<Vec<FileCandidate>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT
            mf.id           AS media_file_id,
            mf.file_path    AS file_path,
            mf.file_hash    AS file_hash,
            mf.runtime_seconds AS runtime_seconds
        FROM media_files mf
        JOIN media_items mi ON mi.id = mf.media_item_id
        WHERE mi.library_id = $1
          AND mi.type IN ('movie', 'episode')
          AND mf.is_healthy = true
          AND NOT EXISTS (
              SELECT 1 FROM storyboards sb
              WHERE sb.media_file_id = mf.id
                AND sb.file_hash = mf.file_hash
          )
        ORDER BY mi.created_at ASC
        "#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| FileCandidate {
            media_file_id: r.get("media_file_id"),
            file_path: r.get("file_path"),
            file_hash: r.get("file_hash"),
            runtime_seconds: r.get("runtime_seconds"),
        })
        .collect())
}

/// Resolve the primary media file for an item (largest healthy file).
/// Mirrors the playback and storyboards domain's selection.
async fn resolve_primary_media_file(
    pool: &PgPool,
    media_item_id: Uuid,
) -> Result<Option<Uuid>, sqlx::Error> {
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

/// Load a single media file's path, hash, and runtime for forced
/// per-item regeneration.
async fn load_single_file_for_generation(
    pool: &PgPool,
    media_file_id: Uuid,
) -> Result<Option<FileCandidate>, sqlx::Error> {
    let row = sqlx::query(
        r#"
        SELECT
            mf.id           AS media_file_id,
            mf.file_path    AS file_path,
            mf.file_hash    AS file_hash,
            mf.runtime_seconds AS runtime_seconds
        FROM media_files mf
        WHERE mf.id = $1 AND mf.is_healthy = true
        "#,
    )
    .bind(media_file_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| FileCandidate {
        media_file_id: r.get("media_file_id"),
        file_path: r.get("file_path"),
        file_hash: r.get("file_hash"),
        runtime_seconds: r.get("runtime_seconds"),
    }))
}

/// Upsert the `storyboards` row after a successful generation. On conflict
/// over `media_file_id`, all fields are refreshed — handles both first-time
/// generation and forced regeneration. The grid shape (`columns`, `rows`)
/// is stored in `metadata` so the domain service can recover it when
/// building sprite URLs.
async fn persist_storyboard_row(
    pool: &PgPool,
    media_file_id: Uuid,
    file_hash: &str,
    cfg: &GenerationConfig,
    result: &sb_svc::GenerationResult,
) -> Result<(), sqlx::Error> {
    let metadata = serde_json::json!({
        "columns": cfg.sprite_columns,
        "rows": cfg.sprite_rows,
    });

    sqlx::query(
        r#"
        INSERT INTO storyboards
            (media_file_id, file_hash, interval_seconds, width, height,
             sprite_count, total_thumbnails, total_size_bytes, keyframe_only,
             quality, generation_duration_ms, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        ON CONFLICT (media_file_id) DO UPDATE
        SET file_hash = EXCLUDED.file_hash,
            interval_seconds = EXCLUDED.interval_seconds,
            width = EXCLUDED.width,
            height = EXCLUDED.height,
            sprite_count = EXCLUDED.sprite_count,
            total_thumbnails = EXCLUDED.total_thumbnails,
            total_size_bytes = EXCLUDED.total_size_bytes,
            keyframe_only = EXCLUDED.keyframe_only,
            quality = EXCLUDED.quality,
            generated_at = now(),
            generation_duration_ms = EXCLUDED.generation_duration_ms,
            metadata = EXCLUDED.metadata,
            updated_at = now()
        "#,
    )
    .bind(media_file_id)
    .bind(file_hash)
    .bind(cfg.interval_seconds as i32)
    .bind(cfg.width as i32)
    .bind(result.height as i32)
    .bind(result.sprite_count as i32)
    .bind(result.total_thumbnails as i32)
    .bind(result.total_size_bytes as i64)
    .bind(cfg.keyframe_only)
    .bind(cfg.quality as i32)
    .bind(result.generation_duration_ms as i32)
    .bind(&metadata)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete any existing storyboard row and on-disk directory for a media
/// file. Used by the per-item "force regen" path before generating fresh.
/// Best-effort: missing rows/files are not errors.
async fn delete_existing_storyboard(pool: &PgPool, media_file_id: Uuid, cache_dir: &Path) {
    let _ = sqlx::query("DELETE FROM storyboards WHERE media_file_id = $1")
        .bind(media_file_id)
        .execute(pool)
        .await;

    let dir = storyboard_dir(cache_dir, media_file_id);
    if let Err(e) = tokio::fs::remove_dir_all(&dir).await
        && e.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(
            media_file_id = %media_file_id,
            dir = %dir.display(),
            error = %e,
            "Failed to clean up existing storyboard directory before regeneration"
        );
    }
}

/// The on-disk directory holding one media file's storyboard artifacts.
/// Layout: `{cache_dir}/storyboards/{media_file_id}/` per STORYBOARDS.md.
fn storyboard_dir(cache_dir: &Path, media_file_id: Uuid) -> std::path::PathBuf {
    cache_dir
        .join(STORYBOARDS_SUBDIR)
        .join(media_file_id.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn library_result_message_format() {
        let r = LibraryGenerationResult {
            candidates: 10,
            generated: 7,
            skipped: 2,
            errors: 1,
        };
        let msg = r.message();
        assert!(msg.contains("10 candidate"));
        assert!(msg.contains("7 generated"));
        assert!(msg.contains("2 skipped"));
        assert!(msg.contains("1 error"));
    }

    #[test]
    fn library_result_default_message() {
        let r = LibraryGenerationResult::default();
        let msg = r.message();
        assert!(msg.contains("0 candidate"));
    }

    #[test]
    fn aggregate_add_accumulates() {
        let mut agg = AggregateResult::default();
        let r1 = LibraryGenerationResult {
            candidates: 5,
            generated: 3,
            skipped: 1,
            errors: 1,
        };
        let r2 = LibraryGenerationResult {
            candidates: 8,
            generated: 6,
            skipped: 2,
            errors: 0,
        };
        agg.add(&r1);
        agg.add(&r2);
        assert_eq!(agg.candidates, 13);
        assert_eq!(agg.generated, 9);
        assert_eq!(agg.skipped, 3);
        assert_eq!(agg.errors, 1);
    }

    #[test]
    fn is_storyboards_enabled_defaults_to_true_when_key_absent() {
        let meta = json!({});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_respects_explicit_false() {
        let meta = json!({"storyboards_enabled": false});
        assert!(!is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_respects_explicit_true() {
        let meta = json!({"storyboards_enabled": true});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn is_storyboards_enabled_ignores_non_bool() {
        let meta = json!({"storyboards_enabled": "yes"});
        assert!(is_storyboards_enabled_for_library(&meta));
        let meta = json!({"storyboards_enabled": 1});
        assert!(is_storyboards_enabled_for_library(&meta));
    }

    #[test]
    fn storyboard_dir_layout() {
        let cache = Path::new("/var/cache/duskcue");
        let id = Uuid::nil();
        let dir = storyboard_dir(cache, id);
        assert_eq!(
            dir,
            std::path::Path::new(
                "/var/cache/duskcue/storyboards/00000000-0000-0000-0000-000000000000"
            )
        );
    }
}
